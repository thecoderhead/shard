use std::io::Write;

use anyhow::{Context, Result};
use crossterm::style::Stylize;
use is_terminal::IsTerminal;

use crate::ui;
use crate::vfs;

pub fn run() -> Result<bool> {
    let mut stdout = std::io::stdout().lock();
    let mut all_ok = true;
    let mut checks: Vec<(&str, bool, String)> = Vec::new();

    writeln!(stdout, "{}", ui::logo())?;
    writeln!(stdout, "{}", ui::hr("system diagnostic"))?;
    writeln!(stdout)?;

    // -- 1. .shard/ writable?
    let root = vfs::root_for_cwd().context("resolve .shard root")?;
    match vfs::ensure_root(&root) {
        Ok(()) => checks.push(("shard root", true, root.display().to_string())),
        Err(e) => {
            checks.push(("shard root", false, format!("{e:#}")));
            all_ok = false;
        }
    }

    // -- 2. SQLite metrics open
    let db_path = vfs::metrics_db_path(&root);
    match crate::metrics::MetricsDb::open(&db_path) {
        Ok(_) => checks.push(("metrics.db", true, db_path.display().to_string())),
        Err(e) => {
            checks.push(("metrics.db", false, format!("{e:#}")));
            all_ok = false;
        }
    }

    // -- 3. TTY detection
    let stdout_tty = std::io::stdout().is_terminal();
    let stdin_tty = std::io::stdin().is_terminal();
    checks.push(("stdout TTY", stdout_tty, if stdout_tty { "yes" } else { "no (piped)" }.into()));
    checks.push(("stdin TTY", stdin_tty, if stdin_tty { "yes" } else { "no (piped)" }.into()));

    // -- 4. PTY spawn dry-run
    let pty_ok = pty_spawn_probe();
    let pty_is_ok = pty_ok.is_ok();
    let backend = if cfg!(windows) { "ConPTY" } else { "Unix98" };
    let pty_detail = match &pty_ok {
        Ok(()) => format!("{backend} ok"),
        Err(e) => format!("{backend} failed: {e}"),
    };
    checks.push(("PTY probe", pty_is_ok, pty_detail));
    if !pty_is_ok {
        all_ok = false;
    }

    // -- 5. SHARD_INTENT
    let intent = crate::intent::from_env();
    let intent_str = match intent {
        Some(ref i) => i.as_str().to_owned(),
        None => "unset".to_owned(),
    };
    checks.push(("SHARD_INTENT", true, intent_str));

    // -- 6. Terminal size
    let dims = terminal_size::terminal_size();
    let dims_str = match dims {
        Some((terminal_size::Width(w), terminal_size::Height(h))) => format!("{w}x{h}"),
        None => "unknown (falling back to 80x24)".to_string(),
    };
    checks.push(("terminal size", true, dims_str));

    // -- Render diagnostic panel
    writeln!(stdout, "{}", ui::box_top())?;
    for (name, ok, detail) in &checks {
        writeln!(stdout, "  {}", ui::status_node(name, *ok, detail))?;
    }
    writeln!(stdout, "{}", ui::box_bottom())?;
    writeln!(stdout)?;

    // -- Summary
    if all_ok {
        writeln!(
            stdout,
            "  {}  {}  {}  {}",
            "◆".green().bold(),
            "SYS::NOMINAL".green().bold(),
            ui::signal_meter(1.0),
            "All systems go.".dim(),
        )?;
        writeln!(
            stdout,
            "  {}  {}",
            "▸".cyan(),
            "shard is ready to intercept.".dim()
        )?;
    } else {
        writeln!(
            stdout,
            "  {}  {}  {}  {}",
            "◆".red().bold(),
            "SYS::FAULT".red().bold(),
            ui::signal_meter(0.15),
            "Some checks failed — review above.".dim(),
        )?;
    }
    writeln!(stdout)?;
    Ok(all_ok)
}

fn pty_spawn_probe() -> std::result::Result<(), String> {
    use portable_pty::{NativePtySystem, PtySize, PtySystem};
    let sys = NativePtySystem::default();
    sys.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}
