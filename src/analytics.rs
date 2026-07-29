use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::Serialize;

use crate::metrics::MetricsDb;
use crate::ui;
use crate::vfs;

/// Aggregation scope for metrics queries.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Scope {
    /// Current project's `.shard/metrics.db`.
    Project,
    /// Global `~/.shard/metrics.db` across all projects.
    Global,
}

pub struct Options {
    pub history: bool,
    pub graph: bool,
    pub since_days: u32,
    pub history_limit: u32,
    pub json: bool,
    pub scope: Scope,
}

pub fn run(opts: Options) -> Result<()> {
    let db_path = resolve_db_path(opts.scope)?;
    if !db_path.exists() {
        if opts.json {
            println!("{{\"total_commands\": 0}}");
        } else {
            print_empty_banner();
        }
        return Ok(());
    }

    let db = MetricsDb::open(&db_path).context("open metrics DB")?;

    if opts.history {
        return print_history(&db, opts.history_limit, opts.json);
    }
    if opts.graph {
        return print_graph(&db, opts.since_days, opts.json);
    }

    print_summary(&db, opts.json)
}

fn resolve_db_path(scope: Scope) -> Result<std::path::PathBuf> {
    match scope {
        Scope::Project => {
            let root = vfs::root_for_cwd()?;
            Ok(vfs::metrics_db_path(&root))
        }
        Scope::Global => {
            let home = dirs::home_dir().context("no home directory found for global scope")?;
            let shard_root = home.join(".shard");
            std::fs::create_dir_all(&shard_root)
                .context("failed to create ~/.shard/ for global metrics")?;
            Ok(shard_root.join("metrics.db"))
        }
    }
}

fn print_empty_banner() {
    println!("{}", ui::logo());
    println!("{}", ui::hr("token savings"));
    println!();
    println!("  {}  {}", "◈".yellow(), "No commands recorded yet.".yellow());
    println!("  {}  {}  {}", "▸".cyan(), "Try:".dim(), "shard echo hello".cyan().bold());
    println!("  {}  {}", "▸".cyan(), "Then run `shard stats` again.".dim());
    println!();
}

fn print_summary(db: &MetricsDb, json: bool) -> Result<()> {
    let s = db.summary().context("summary query")?;
    if json {
        let out = SummaryJson {
            total_commands: s.total_commands,
            input_tokens: s.tokens_in,
            output_tokens: s.tokens_out,
            tokens_saved: s.tokens_saved,
            savings_pct: s.savings_pct,
            total_exec_ms: s.total_exec_ms,
            avg_exec_ms: s.avg_exec_ms,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{}", ui::logo());
    println!("{}", ui::hr("token savings"));
    println!();
    println!("{}", ui::box_top());
    println!("{}", ui::data_row("Commands", &ui::fmt_num(s.total_commands)));
    println!("{}", ui::data_row("Input tokens", &ui::fmt_num(s.tokens_in)));
    println!("{}", ui::data_row("Output tokens", &ui::fmt_num(s.tokens_out)));
    println!("{}", ui::data_row_green("Saved", &format!("{}  ({:.1}%)", ui::fmt_num(s.tokens_saved), s.savings_pct)));
    println!("{}", ui::data_row("Wall time", &format!("{}  (avg {})", ui::fmt_duration(s.total_exec_ms), ui::fmt_duration(s.avg_exec_ms))));
    println!("{}", ui::data_row("Signal", &ui::signal_meter(s.savings_pct / 100.0)));
    println!("{}", ui::box_bottom());
    println!();
    println!("{}", ui::token_histogram(s.tokens_in, s.tokens_out, 30));
    println!();
    Ok(())
}

fn print_history(db: &MetricsDb, limit: u32, json: bool) -> Result<()> {
    let rows = db.recent(limit).context("history query")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows.iter().map(|r| {
            serde_json::json!({
                "id": r.id,
                "started_at": r.started_at,
                "argv": serde_json::from_str::<serde_json::Value>(&r.argv_json).unwrap_or(serde_json::Value::Null),
                "savings_pct": r.savings_pct,
                "tokens_saved": r.tokens_saved,
                "tokens_in": r.tokens_in,
                "tokens_out": r.tokens_out,
                "wall_ms": r.wall_ms,
                "exit_code": r.exit_code,
                "log_path": r.log_path,
            })
        }).collect::<Vec<_>>())?);
        return Ok(());
    }
    println!("{}", ui::logo());
    println!("{}", ui::hr("command history"));
    println!();
    if rows.is_empty() {
        println!("  {}  {}", "◈".yellow(), "(no runs recorded)".yellow());
        println!();
        return Ok(());
    }
    for r in rows {
        let argv_pretty: Vec<String> =
            serde_json::from_str(&r.argv_json).unwrap_or_else(|_| vec![r.argv_json.clone()]);
        let marker = if r.savings_pct >= 60.0 {
            "▸".green().bold().to_string()
        } else if r.savings_pct >= 20.0 {
            "▸".yellow().bold().to_string()
        } else {
            "▸".red().to_string()
        };
        let cmd = format!("shard {}", argv_pretty.join(" "));
        let cmd = if cmd.len() > 40 {
            let mut s: String = cmd.chars().take(37).collect();
            s.push_str("...");
            s
        } else {
            cmd
        };
        let ts = r.started_at.get(5..16).unwrap_or(r.started_at.as_str());
        println!(
            "  {ts}  {marker}  {cmd:<40}  {pct:>6}  {saved}",
            pct = format!("{:.1}%", r.savings_pct).bold(),
            saved = format!("{} tk", ui::fmt_num(r.tokens_saved)).bold(),
        );
    }
    println!();
    Ok(())
}

fn print_graph(db: &MetricsDb, days: u32, json: bool) -> Result<()> {
    let rows = db.daily(days).context("daily query")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows.iter().map(|r| {
            serde_json::json!({
                "day": r.day,
                "runs": r.runs,
                "tokens_in": r.tokens_in,
                "tokens_out": r.tokens_out,
                "tokens_saved": r.tokens_in.saturating_sub(r.tokens_out),
            })
        }).collect::<Vec<_>>())?);
        return Ok(());
    }
    println!("{}", ui::logo());
    println!("{}", ui::hr(&format!("daily savings — last {days} days")));
    println!();
    if rows.is_empty() {
        println!("  {}  {}", "◈".yellow(), "(no runs in window)".yellow());
        println!();
        return Ok(());
    }
    let max = rows
        .iter()
        .map(|r| r.tokens_in.saturating_sub(r.tokens_out))
        .max()
        .unwrap_or(1)
        .max(1);
    for r in rows {
        let saved = r.tokens_in.saturating_sub(r.tokens_out);
        let fraction = saved as f64 / max as f64;
        let bar = ui::progress_bar(fraction, 40);
        println!(
            "  {}  {}{}  {}  {} {}",
            r.day,
            bar,
            if fraction >= 0.95 { " ⚡" } else { "  " },
            format!("{} saved", ui::fmt_num(saved)).bold(),
            format!("({} runs)", r.runs).dim(),
            if fraction >= 0.95 { "PEAK".cyan().bold().to_string() } else { String::new() },
        );
    }
    println!();
    Ok(())
}

#[derive(Serialize)]
struct SummaryJson {
    total_commands: u64,
    input_tokens: u64,
    output_tokens: u64,
    tokens_saved: u64,
    savings_pct: f64,
    total_exec_ms: u64,
    avg_exec_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_formatting() {
        assert_eq!(ui::fmt_num(0), "0");
        assert_eq!(ui::fmt_num(999), "999");
        assert_eq!(ui::fmt_num(1_000), "1,000");
        assert_eq!(ui::fmt_num(1_234_567), "1,234,567");
    }

    #[test]
    fn efficiency_bar_contains_pct() {
        let bar = ui::progress_bar(0.0, 24);
        assert!(bar.contains("0.0%"), "zero bar has 0.0%  got: {bar:?}");
        let bar = ui::progress_bar(1.0, 24);
        assert!(bar.contains("100.0%"), "full bar has 100.0%  got: {bar:?}");
    }
}
