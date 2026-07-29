use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossterm::style::Stylize;

use crate::ui;
use crate::vfs;

pub struct LogOptions {
    pub prefix: String,
    pub raw: bool,
    pub intent: Option<String>,
}

pub fn run(opts: LogOptions) -> Result<()> {
    let root = vfs::root_for_cwd()?;
    let logs_dir = root.join(vfs::LOGS_SUBDIR);

    if !logs_dir.exists() {
        println!(
            "  {}  {}",
            "◈".dim(),
            "No logs directory found. Run a command through shard first.".yellow()
        );
        return Ok(());
    }

    let candidates = find_log_by_prefix(&logs_dir, &opts.prefix)?;

    if candidates.is_empty() {
        println!(
            "  {}  {} {}",
            "●".red(),
            "No log found matching prefix:".red(),
            opts.prefix.cyan()
        );
        return Ok(());
    }

    if candidates.len() > 1 {
        println!("  {}  {} {}", "◈".yellow(), "Multiple logs match prefix:".yellow(), opts.prefix.cyan());
        for (uuid, path) in &candidates {
            let meta = path.metadata().ok();
            let size = meta.map(|m| m.len()).unwrap_or(0);
            println!("  {} {} ({})", "▸".dim(), uuid.clone().cyan(), human_size(size));
        }
        return Ok(());
    }

    let (_uuid, path) = &candidates[0];
    let raw = fs::read(path)
        .with_context(|| format!("Failed to read log at {}", path.display()))?;

    if opts.raw {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        out.write_all(&raw)?;
        out.flush()?;
        return Ok(());
    }

    // Strip ANSI for compaction
    let plain_text = strip_ansi(&raw);

    let intent = opts.intent.as_deref().map(|s| {
        if s == "debug:test-failure" {
            crate::intent::Intent::DebugTestFailure
        } else if s == "commit:generate" {
            crate::intent::Intent::CommitGenerate
        } else {
            crate::intent::Intent::Custom(s.to_owned())
        }
    });

    let compacted = crate::compact::compact(
        &String::from_utf8_lossy(&plain_text),
        intent.as_ref(),
    );

    let tokens_in = crate::tokens::approx(&plain_text);
    let tokens_out = crate::tokens::approx(compacted.text.as_bytes());
    let savings = crate::tokens::savings_pct(tokens_in, tokens_out);

    // Display header
    println!("{}", ui::logo());
    println!("{}", ui::hr(&format!("replay {}", candidates[0].0)));
    println!();
    println!("{}", ui::box_top());
    println!("{}", ui::data_row("Archetype", &compacted.archetype.as_str()));
    println!("{}", ui::data_row("Tokens", &format!("{} → {}", ui::fmt_num(tokens_in), ui::fmt_num(tokens_out).green().bold())));
    println!("{}", ui::data_row_green("Saved", &format!("{} ({:.1}%)", ui::fmt_num(tokens_in.saturating_sub(tokens_out)), savings)));
    println!("{}", ui::data_row("Signal", &ui::signal_meter(savings / 100.0)));
    println!("{}", ui::box_bottom());
    println!();

    // Compacted body
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{}", ui::divider())?;
    out.write_all(compacted.text.as_bytes())?;
    if !compacted.text.ends_with('\n') {
        writeln!(out)?;
    }
    writeln!(out, "{}", ui::divider())?;
    out.flush()?;

    Ok(())
}

fn find_log_by_prefix(logs_dir: &PathBuf, prefix: &str) -> Result<Vec<(String, PathBuf)>> {
    let mut matches = Vec::new();
    if !logs_dir.exists() {
        return Ok(matches);
    }
    let entries = fs::read_dir(logs_dir)
        .with_context(|| "Failed to read logs directory")?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.starts_with(prefix) {
                matches.push((stem.to_owned(), path));
            }
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(matches)
}

fn strip_ansi(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut in_escape = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            in_escape = true;
            i += 1;
            continue;
        }
        if in_escape {
            if bytes[i] == 0x5b || bytes[i] == 0x5d {
                i += 1;
                continue;
            }
            if bytes[i] >= 0x40 && bytes[i] <= 0x7E {
                in_escape = false;
                i += 1;
                continue;
            }
            if bytes[i] == 0x07 || (bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == 0x5c) {
                in_escape = false;
                if bytes[i] == 0x1b {
                    i += 1;
                }
                i += 1;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
