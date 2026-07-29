use std::fs;

use anyhow::Result;

use crate::ui;
use crate::vfs;

pub fn run() -> Result<()> {
    let root = vfs::root_for_cwd()?;

    println!("{}", ui::logo());
    println!("{}", ui::hr("clean"));
    println!();

    let mut cleaned = 0u64;
    let mut freed = 0u64;

    // Phase 1: purge log files
    let logs_dir = root.join(vfs::LOGS_SUBDIR);
    if logs_dir.exists() {
        if let Ok(entries) = fs::read_dir(&logs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("log") {
                    if let Ok(meta) = entry.metadata() {
                        freed += meta.len();
                    }
                    if fs::remove_file(&path).is_ok() {
                        cleaned += 1;
                    }
                }
            }
        }
    }

    // Phase 2: VACUUM metrics DB
    let db_path = vfs::metrics_db_path(&root);
    if db_path.exists() {
        if let Ok(db) = crate::metrics::MetricsDb::open(&db_path) {
            if let Ok(size_before) = db_path.metadata().map(|m| m.len()) {
                db.vacuum().ok();
                if let Ok(size_after) = db_path.metadata().map(|m| m.len()) {
                    freed = freed.saturating_add(size_before.saturating_sub(size_after));
                }
            }
        }
    }

    println!("{}", ui::box_top());
    println!("{}", ui::data_row("Logs purged", &ui::fmt_num(cleaned)));
    println!("{}", ui::data_row_green("Space freed", &human_size(freed)));
    println!("{}", ui::box_bottom());
    println!();
    println!("{}", ui::ok("Shard cache cleaned successfully."));
    Ok(())
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
