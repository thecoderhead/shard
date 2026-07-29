//! `ShardPTYBridge` implementation. See parent module docs for architecture.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use chrono::Utc;
use is_terminal::IsTerminal;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use uuid::Uuid;

use crate::compact::streaming::StreamingCompactor;
use crate::compact::Archetype;
use crate::intent;
use crate::metrics::RunRecord;
use crate::tokens;
use crate::vfs::cache::{self as vfs_cache, RawLog};
use crate::vfs::{self, LOGS_SUBDIR};
use crate::vte_tok::{Token, Tokenizer};

/// Configuration for a single Shard-mediated run.
pub struct ShardPTYBridge {
    argv: Vec<String>,
    cwd: PathBuf,
    metrics_db_path: PathBuf,
    logs_dir: PathBuf,
    intent: Option<intent::Intent>,
    initial_cols: u16,
    initial_rows: u16,
    parent_stdout_is_tty: bool,
    parent_stdin_is_tty: bool,
}

/// Result of a completed run. Ownership handed back to the CLI dispatcher which
/// forwards `exit_code` as the process exit code.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields are consumed by Phase 2 compaction footer + shard gain UI.
pub struct RunOutcome {
    pub run_id: Uuid,
    pub exit_code: i32,
    pub wall_ms: u64,
    pub raw_bytes: u64,
    pub log_path: PathBuf,
    pub is_tty: bool,
    pub passthrough_only: bool,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub savings_pct: f64,
}

impl ShardPTYBridge {
    /// Build a bridge for `argv` in the current working directory.
    pub fn new(argv: Vec<String>) -> Result<Self> {
        if argv.is_empty() {
            return Err(anyhow!("empty argv passed to ShardPTYBridge"));
        }

        let cwd = std::env::current_dir().context("resolve cwd")?;
        let shard_root = cwd.join(vfs::ROOT_DIR);
        vfs::ensure_root(&shard_root).context("ensure .shard/ directory tree")?;

        let (cols, rows) = detect_terminal_size();
        let parent_stdout_is_tty = std::io::stdout().is_terminal();
        let parent_stdin_is_tty = std::io::stdin().is_terminal();

        Ok(Self {
            argv,
            cwd: cwd.clone(),
            metrics_db_path: vfs::metrics_db_path(&shard_root),
            logs_dir: shard_root.join(LOGS_SUBDIR),
            intent: intent::from_env(),
            initial_cols: cols,
            initial_rows: rows,
            parent_stdout_is_tty,
            parent_stdin_is_tty,
        })
    }

    /// Execute the command. Returns after the child exits (or is signalled).
    pub async fn run(self) -> Result<RunOutcome> {
        let run_id = Uuid::new_v4();
        let started_at = Utc::now();
        let start_instant = Instant::now();

        let raw_log = RawLog::allocate(&self.logs_dir, run_id);
        let log_path = raw_log.path.clone();

        let (vfs_tx, vfs_rx) = unbounded_channel::<Bytes>();
        let (vte_tx, vte_rx) = unbounded_channel::<Bytes>();

        let cancel = Arc::new(tokio::sync::Notify::new());

        let compaction_mode = !self.parent_stdout_is_tty;

        // Path B sink #1 — raw log VFS cache.
        let vfs_path = log_path.clone();
        let vfs_cancel = cancel.clone();
        let vfs_task = tokio::spawn(async move {
            vfs_cache::drain_to_file(vfs_path, vfs_rx, vfs_cancel).await
        });

        // Path B sink #2 — VTE tokenizer. Categorises bytes into
        // Sgr/Text/Control and accumulates the plain-text bytes for the
        // compaction engine. In piped mode, also streams compacted output
        // incrementally through a StreamingCompactor.
        let (compact_tx, compact_rx) = unbounded_channel::<String>();

        let vte_cancel = cancel.clone();
        let compact_tx_option = if compaction_mode {
            Some(compact_tx)
        } else {
            drop(compact_rx);
            None
        };
        let vte_task = tokio::spawn(async move {
            drain_vte(vte_rx, vte_cancel, compact_tx_option).await
        });

        // Spawn a writer task that consumes compacted windows from the
        // streaming compactor and writes them to stdout as they arrive.
        let compact_writer = if compaction_mode {
            Some(tokio::task::spawn_blocking(|| {
                let stdout = std::io::stdout();
                let mut buf = stdout.lock();
                while let Some(chunk) = compact_rx.blocking_recv() {
                    use std::io::Write as _;
                    let _ = buf.write_all(chunk.as_bytes());
                    let _ = buf.flush();
                }
            }))
        } else {
            None
        };

        let outcome = self
            .spawn_and_tee(run_id, vfs_tx, vte_tx, start_instant, cancel.clone())
            .await?;

        let raw_bytes_written = vfs_task
            .await
            .context("vfs task join failed")?
            .context("vfs task returned error")?;
        let vte_result = vte_task.await.context("vte task join failed")?;

        let logs_dir = self.logs_dir.clone();
        tokio::task::spawn_blocking(move || vfs_cache::rotate(&logs_dir)).await
            .context("log rotation task join failed")?;

        let finished_at = Utc::now();
        let wall_ms = start_instant.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        // Wait for the streaming compact-writer task to finish.
        // This ensures all compacted windows have been flushed to stdout
        // before we emit the footer.
        if let Some(handle) = compact_writer {
            let _ = handle.await;
        }

        // Compaction step (piped mode only). In streaming mode the compacted
        // text was already emitted incrementally — we only emit the footer
        // here. In TTY mode skip compaction and record parity savings.
        let compaction_mode = !self.parent_stdout_is_tty;
        let plain_text = vte_result.plain_text;
        let streamed_arch = vte_result.streamed_archetype;

        let (tokens_in, tokens_out, savings_pct, archetype_str) = if compaction_mode {
            let tin = tokens::approx(plain_text.as_bytes());
            // Re-count output tokens: in streaming mode we don't have the
            // final compacted string handy, so we run a quick synchronous
            // pass on the full text just for the footer counters.
            let compacted = crate::compact::compact(&plain_text, self.intent.as_ref());
            let tout = tokens::approx(compacted.text.as_bytes());
            let pct = tokens::savings_pct(tin, tout);
            let arch_name = streamed_arch
                .as_ref()
                .map(|a| a.as_str().to_owned())
                .unwrap_or_else(|| compacted.archetype.as_str().to_owned());
            let fmt = crate::output_format::OutputFormat::from_env();
            let footer = crate::output_format::format_footer(
                fmt, tin, tout, pct, compacted.archetype, &log_path,
            );
            use std::io::Write as _;
            let _ = std::io::stdout().lock().write_all(footer.as_bytes());
            let _ = std::io::stdout().lock().flush();
            (tin, tout, pct, arch_name)
        } else {
            let tin = tokens::approx_from_bytes(raw_bytes_written);
            (tin, tin, 0.0, crate::compact::Archetype::Passthrough.as_str().to_owned())
        };

        let record = RunRecord {
            id: run_id,
            argv: self.argv.clone(),
            cwd: self.cwd.clone(),
            started_at,
            finished_at,
            wall_ms,
            exit_code: outcome.exit_code,
            raw_bytes: raw_bytes_written,
            tokens_in,
            tokens_out,
            savings_pct,
            log_path: log_path.clone(),
            intent: self.intent.as_ref().map(|i| i.as_str().to_owned()),
            is_tty: self.parent_stdout_is_tty,
        };

        use crate::metrics;
        let db_path = self.metrics_db_path.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(db) = metrics::open_or_degrade(&db_path) {
                if let Err(e) = db.insert_run(&record) {
                    tracing::warn!(target: "shard::metrics", %e, "failed to insert run record");
                }
            }
        })
        .await;

        tracing::debug!(
            target: "shard::pty",
            archetype = %archetype_str,
            wall_ms,
            raw_bytes = raw_bytes_written,
            tokens_in,
            tokens_out,
            "run complete"
        );

        Ok(RunOutcome {
            run_id,
            exit_code: outcome.exit_code,
            wall_ms,
            raw_bytes: raw_bytes_written,
            log_path,
            is_tty: self.parent_stdout_is_tty,
            passthrough_only: self.parent_stdout_is_tty,
            tokens_in,
            tokens_out,
            savings_pct,
        })
    }

    /// Spawn the PTY child, wire up Path A (raw passthrough) and Path B
    /// (analysis fan-out via `vfs_tx` and `vte_tx`), and wait for the child.
    async fn spawn_and_tee(
        &self,
        _run_id: Uuid,
        vfs_tx: UnboundedSender<Bytes>,
        vte_tx: UnboundedSender<Bytes>,
        _start_instant: Instant,
        cancel: Arc<tokio::sync::Notify>,
    ) -> Result<InnerOutcome> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: self.initial_rows,
                cols: self.initial_cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .with_context(|| format!("openpty failed for argv={:?}", self.argv))?;

        let mut cmd = CommandBuilder::new(&self.argv[0]);
        for arg in &self.argv[1..] {
            cmd.arg(arg);
        }
        cmd.cwd(&self.cwd);
        // Inherit the parent environment. On Windows we skip the cmd-internal
        // "drive-tracking" vars whose names begin with '=' (e.g. `=C:`);
        // CreateProcess accepts them but portable-pty's env stringification
        // has historically choked on them and, more importantly, cmd.exe
        // interprets them specially and can hang if they're malformed.
        //
        // We also strip `SHARD_HOOK_ACTIVE` so the child process starts
        // without hook aliases — if we pass it through, the child (e.g. git
        // invoked via `shard git status`) would recursively call `shard git
        // status`, creating an infinite loop.
        const HOOK_ACTIVE: &str = "SHARD_HOOK_ACTIVE";
        for (k, v) in std::env::vars_os() {
            let key_bytes = k.to_string_lossy();
            if key_bytes.starts_with('=') || key_bytes.as_ref() == HOOK_ACTIVE {
                continue;
            }
            cmd.env(k, v);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("spawn failed for argv={:?}", self.argv))?;

        // Drop the slave end in the parent — the child owns it now. This is
        // required on some platforms for EOF propagation when the child exits.
        drop(pair.slave);

        // The `master` end gives us a reader (child -> parent) and a writer
        // (parent -> child). We always take both — on Windows ConPTY, closing
        // the master's input handle mid-run is interpreted as "terminal
        // disconnected" and causes ConPTY to kill the child with
        // STATUS_CONTROL_C_EXIT (0xC000013A). The writer must therefore stay
        // alive for the full lifetime of the child. In the TTY case we hand it
        // off to a stdin-forwarder thread; in the piped case we bind it to a
        // scope-held guard so it drops at end-of-run.
        let mut master_reader = pair
            .master
            .try_clone_reader()
            .context("clone PTY master reader")?;
        let master_writer = pair
            .master
            .take_writer()
            .context("take PTY master writer")?;

        let shutdown = Arc::new(AtomicBool::new(false));

        // Compaction mode gate:
        //
        // - TTY-attached parent (developer is watching):       raw passthrough.
        //   The AI never sees this stream; the developer needs full fidelity
        //   for TUI apps like `vim` and `less`.
        // - Piped/redirected parent (AI agent is capturing):   suppress raw
        //   stdout in the reader, buffer child text via the VTE tokenizer
        //   (Path B), and emit the compacted output + savings footer after
        //   the child exits. This is where the 60-90% token savings live.
        let compaction_mode = !self.parent_stdout_is_tty;

        // -- Reader thread (child -> parent): Path A + Path B tee. ---------
        //
        // The reader relies primarily on EOF from the PTY master (child exit
        // -> ConPTY / master fd closes -> read returns 0) to terminate. We
        // deliberately do NOT gate the loop on `shutdown` for correctness on
        // Unix, where there's a measurable delay between `child.wait()`
        // returning and the final child output reaching the master pipe.
        //
        // On Windows ConPTY the read-side clone sometimes does *not* observe
        // EOF even after the child exits and the master is dropped; the main
        // thread signals a drain deadline via `reader_done_tx`, and if the
        // reader hasn't drained by then, we abandon it (the OS thread dies
        // when our process exits).
        let (reader_done_tx, reader_done_rx) = tokio::sync::oneshot::channel::<()>();
        let vfs_tx_reader = vfs_tx.clone();
        let vte_tx_reader = vte_tx.clone();
        let passthrough_stdout = !compaction_mode;
        let _reader_join = thread::Builder::new()
            .name("shard-pty-reader".into())
            .spawn(move || -> std::io::Result<()> {
                let stdout = std::io::stdout();
                let mut stdout = stdout.lock();
                let mut buf = [0u8; 8192];
                let mut total_read: u64 = 0;
                let result = loop {
                    match master_reader.read(&mut buf) {
                        Ok(0) => break Ok(()), // EOF — child closed the PTY.
                        Ok(n) => {
                            total_read += n as u64;
                            if passthrough_stdout {
                                // Path A — raw pass-through, flushed
                                // immediately so interactive prompts appear
                                // the instant they're emitted.
                                if let Err(e) = stdout.write_all(&buf[..n]) {
                                    break Err(e);
                                }
                                if let Err(e) = stdout.flush() {
                                    break Err(e);
                                }
                            }
                            // Path B — always active. Ref-counted `Bytes`
                            // makes the second clone pointer-sized.
                            let chunk = Bytes::copy_from_slice(&buf[..n]);
                            let _ = vfs_tx_reader.send(chunk.clone());
                            let _ = vte_tx_reader.send(chunk);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) if is_pty_closed(&e) => break Ok(()),
                        Err(e) => break Err(e),
                    }
                };
                tracing::debug!(target: "shard::pty", total_read, "reader finished");
                let _ = reader_done_tx.send(());
                result
            })
            .context("spawn shard-pty-reader thread")?;




        // -- Writer thread (parent stdin -> PTY master). -----------------
        //
        // TTY path: move `master_writer` into a dedicated forwarder thread.
        // Piped path: bind `master_writer` to a scope-held guard so it lives
        // until end-of-run and is not dropped early (see the note above
        // `take_writer` for why).
        let (writer_join, _writer_guard) = if self.parent_stdin_is_tty {
            let mut master_writer = master_writer;
            let shutdown_writer = shutdown.clone();
            let handle = thread::Builder::new()
                .name("shard-pty-writer".into())
                .spawn(move || -> std::io::Result<()> {
                    let stdin = std::io::stdin();
                    let mut stdin = stdin.lock();
                    let mut buf = [0u8; 4096];
                    loop {
                        if shutdown_writer.load(Ordering::Relaxed) {
                            break;
                        }
                        match stdin.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                if let Err(e) = master_writer.write_all(&buf[..n]) {
                                    if is_pty_closed(&e) {
                                        break;
                                    }
                                    return Err(e);
                                }
                                if let Err(e) = master_writer.flush() {
                                    if is_pty_closed(&e) {
                                        break;
                                    }
                                    return Err(e);
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) if is_pty_closed(&e) => break,
                            Err(e) => return Err(e),
                        }
                    }
                    Ok(())
                })
                .context("spawn shard-pty-writer thread")?;
            (Some(handle), None)
        } else {
            // Guard binding: keep the writer alive so ConPTY doesn't kill the
            // child. It'll drop at end of scope, after we've collected the
            // child's exit status.
            (None, Some(master_writer))
        };

        // -- Child wait (poll) on a blocking worker. ----------------------
        //
        // We poll `try_wait()` at ~40 Hz on a blocking thread instead of
        // calling the blocking `wait()`. On Windows ConPTY, `wait()` has been
        // observed to hang forever in some detached-parent-console scenarios;
        // `try_wait` uses `GetExitCodeProcess`, which is a plain state query
        // and returns reliably.
        let wait_task = tokio::task::spawn_blocking(move || -> Result<portable_pty::ExitStatus, std::io::Error> {
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return Ok(status),
                    Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                    Err(e) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()));
                    }
                }
            }
        });

        // Poll for Ctrl+C alongside child wait.
        //
        // We *don't* eat the signal — the raw passthrough already delivered
        // Ctrl+C bytes into the PTY (if stdin is a TTY), so most child
        // processes will handle it. The tokio ctrl_c handler is a
        // belt-and-braces measure for the piped-stdin case.
        //
        // Pattern-match on `Ok(())` so that a signal-registration failure
        // (e.g. Windows subprocess without an attached console) disables the
        // branch instead of resolving immediately with `Err(_)` — otherwise
        // the interruption path fires as soon as the future is polled.
        let exit_status = tokio::select! {
            biased;
            res = wait_task => res.context("child wait join failed")?.context("child wait failed")?,
            Ok(()) = tokio::signal::ctrl_c() => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                shutdown.store(true, Ordering::Relaxed);
                return Err(anyhow!("interrupted"));
            }
        };

        // The child has exited. Now it's safe to drop the writer guard
        // (piped-stdin case) — ConPTY won't misinterpret the closed input as
        // "terminal disconnected" because there's no live child to signal.
        drop(_writer_guard);

        // Drop the master owner so ConPTY closes the output pipe and the
        // reader can observe EOF. The reader's cloned handle keeps the read
        // side alive until its own drain completes.
        drop(pair.master);

        // Wait for the reader to fully drain any bytes still in flight through
        // ConPTY. On Windows ConPTY, the cloned read handle occasionally
        // fails to observe EOF even after the child exits and the master
        // drops; cap the drain wait so a stuck reader can't hang the process.
        // Bytes already delivered before the deadline reach the sinks
        // correctly; any post-deadline tail is lost, which is acceptable
        // given that the primary raw output (Path A) has already reached the
        // user's terminal.
        let _ = tokio::time::timeout(Duration::from_millis(1_500), reader_done_rx).await;

        // Signal the Path B sinks that no further bytes will arrive. This
        // covers the case where the reader thread is still stuck on a
        // blocking read: sinks see the notification and drain their buffered
        // messages instead of waiting for the reader's sender clones to drop.
        cancel.notify_waiters();

        // Best-effort drop of the outer sender halves — combined with the
        // notification above this is sufficient for the sinks to exit.
        shutdown.store(true, Ordering::Relaxed);
        drop(vfs_tx);
        drop(vte_tx);

        // Writer thread joined implicitly via drop; on Windows the blocked
        // read on stdin will unwind when the process exits.
        drop(writer_join);

        // portable-pty ExitStatus doesn't expose a signed exit code directly;
        // `exit_code()` returns `u32`, which we clamp into `i32` semantics
        // matching Unix wait status.
        let raw = exit_status.exit_code();
        tracing::debug!(target: "shard::pty", raw_exit = raw, "child exited");
        let exit_code = if raw > i32::MAX as u32 {
            // Unusual — but preserve LSB info.
            (raw as i32).max(1)
        } else {
            raw as i32
        };

        Ok(InnerOutcome { exit_code })
    }
}

struct InnerOutcome {
    exit_code: i32,
}

#[derive(Debug, Default)]
struct VteResult {
    plain_text: String,
    /// Archetype selected by the streaming classifier, if piped mode.
    streamed_archetype: Option<Archetype>,
    #[allow(dead_code)]
    counts: TokenCounts,
}

#[derive(Debug, Clone, Default)]
struct TokenCounts {
    text_events: u64,
    text_bytes: u64,
    sgr_events: u64,
    sgr_bytes: u64,
    control_events: u64,
    control_bytes: u64,
}

/// VTE tokenizer sink task. Consumes bytes from `rx`, feeds them through the
/// state machine, and accumulates the `Text` tokens into a UTF-8 string. The
/// resulting text is what the compaction engine operates on — ANSI colours
/// and control sequences are already stripped by construction.
///
/// Uses `RefCell` instead of `Arc<Mutex>` because this runs on a single
/// tokio task (no cross-thread contention). Also uses `from_utf8_unchecked`
/// on token byte slices — the VTE parser emits valid UTF-8 from printable
/// characters, so the extra `Cow` from `from_utf8_lossy` is wasted work.
///
/// When `compact_tx` is provided (piped mode), text is streamed through a
/// [`StreamingCompactor`] and compacted windows are sent incrementally so
/// the AI sees output long before the child exits.
async fn drain_vte(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>,
    cancel: Arc<tokio::sync::Notify>,
    compact_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> VteResult {
    let counts = Arc::new(std::sync::Mutex::new(TokenCounts::default()));
    let text = Arc::new(std::sync::Mutex::new(String::with_capacity(4096)));
    let mut stream = compact_tx.is_some().then(|| StreamingCompactor::new(100));
    let mut streamed_archetype: Option<Archetype> = None;

    let counts_c = counts.clone();
    let text_c = text.clone();
    let mut tok = Tokenizer::new(move |t| {
        match t {
            Token::Text(b) => {
                let mut c = counts_c.lock().unwrap();
                c.text_bytes += b.len() as u64;
                c.text_events += 1;
                let s = String::from_utf8(b).unwrap_or_default();
                text_c.lock().unwrap().push_str(&s);
            }
            Token::Sgr { bytes } => {
                let mut c = counts_c.lock().unwrap();
                c.sgr_bytes += bytes.len() as u64;
                c.sgr_events += 1;
            }
            Token::Control { bytes } => {
                let mut c = counts_c.lock().unwrap();
                c.control_bytes += bytes.len() as u64;
                c.control_events += 1;
            }
        }
    });

    let sender = compact_tx;
    let cancel_notified = cancel.notified();
    tokio::pin!(cancel_notified);
    loop {
        let chunk: Option<Bytes> = tokio::select! {
            biased;
            msg = rx.recv() => msg,
            _ = &mut cancel_notified => {
                while let Ok(chunk) = rx.try_recv() {
                    tok.feed(&chunk);
                }
                None
            }
        };
        match chunk {
            Some(bytes) => {
                let text_len_before = text.lock().unwrap().len();
                tok.feed(&bytes);
                let text_len_after = text.lock().unwrap().len();
                if text_len_after > text_len_before {
                    let delta = text.lock().unwrap()[text_len_before..].to_owned();
                    if let Some(ref mut s) = stream {
                        if let Some(ref tx) = sender {
                            for w in s.feed_text(&delta) {
                                let _ = tx.send(w);
                            }
                            if let Some(a) = s.archetype() {
                                streamed_archetype = Some(a);
                            }
                        }
                    }
                }
            }
            None => break,
        }
    }
    tok.finish();

    let final_text = text.lock().unwrap().clone();
    let final_counts = counts.lock().unwrap().clone();

    // Flush remaining compacted windows.
    if let Some(ref mut s) = stream {
        if let Some(ref tx) = sender {
            for w in s.feed_text(&final_text) {
                let _ = tx.send(w);
            }
            if let Some(a) = s.archetype() {
                streamed_archetype = Some(a);
            }
            if let Some(tail) = s.flush() {
                let _ = tx.send(tail);
            }
        }
    }

    tracing::debug!(target: "shard::vte", ?final_counts, archetype = ?streamed_archetype, "vte tokenizer finished");
    VteResult { plain_text: final_text, streamed_archetype, counts: final_counts }
}

/// Detect terminal size for the PTY child. Falls back to 80x24 (the classic
/// vt100 default) when there's no attached TTY (piped mode).
fn detect_terminal_size() -> (u16, u16) {
    if let Some((terminal_size::Width(w), terminal_size::Height(h))) = terminal_size::terminal_size()
    {
        (w.max(20), h.max(5))
    } else {
        (80, 24)
    }
}

/// On Windows in particular, the PTY master returns various flavours of
/// `BrokenPipe` / `ConnectionAborted` when the child exits. Treat all of them
/// as clean EOF so we don't spuriously fail a successful run.
fn is_pty_closed(err: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        err.kind(),
        BrokenPipe | UnexpectedEof | ConnectionAborted | ConnectionReset
    )
}
