use std::io::Write;

use anyhow::Result;
use crossterm::style::Stylize;

use crate::hooks_impl;
use crate::hooks_impl::registry::{EditOutcome, remove_block};
use crate::ui;

pub struct InitOptions {
    pub global: bool,
    pub show: bool,
    pub uninstall: bool,
    pub context_prune: bool,
    pub cat_compress: bool,
}

pub fn run(opts: InitOptions) -> Result<()> {
    let mut stdout = std::io::stdout().lock();

    if opts.show {
        let report = hooks_impl::registry::run_show(opts.global)?;
        writeln!(stdout, "{}", ui::logo())?;
        writeln!(stdout, "{}", ui::hr("hook status"))?;
        writeln!(stdout)?;
        if report.is_empty() {
            writeln!(stdout, "  {}  {}", "◈".yellow(), "No target files discovered on this host.".yellow())?;
            writeln!(stdout)?;
            return Ok(());
        }
        writeln!(stdout, "{}", ui::box_top())?;
        for row in report {
            let mark = if row.installed {
                "●".green().bold().to_string()
            } else {
                "○".dim().to_string()
            };
            writeln!(stdout, "  {mark}  {:<22}  {}", row.target.bold(), row.path.dim())?;
        }
        writeln!(stdout, "{}", ui::box_bottom())?;
        writeln!(stdout)?;
        return Ok(());
    }

    if opts.uninstall {
        let report = hooks_impl::registry::run_uninstall(opts.global)?;
        let home = dirs::home_dir();
        let features_outcome = home.as_ref().and_then(|h| {
            let p = h.join(".shard").join("features.sh");
            remove_block(&p).ok()
        });

        writeln!(stdout, "{}", ui::logo())?;
        writeln!(stdout, "{}", ui::hr("uninstall"))?;
        writeln!(stdout)?;
        writeln!(stdout, "{}", ui::box_top())?;
        for row in report {
            let (symbol, outcome_str) = format_outcome(&row.outcome);
            writeln!(stdout, "  {}  {} {:<22}  {}", symbol, outcome_str, row.target.bold(), row.path.dim())?;
        }
        if let Some(outcome) = features_outcome {
            let (symbol, outcome_str) = format_outcome(&outcome);
            writeln!(stdout, "  {}  {} {:<22}  {}", symbol, outcome_str, "shard-features".bold(), "~/.shard/features.sh".dim())?;
        }
        writeln!(stdout, "{}", ui::box_bottom())?;
        writeln!(stdout)?;
        writeln!(stdout, "  {}  {}", "▸".dim(), "Hooks removed. Restart your shell to apply.".dim())?;
        writeln!(stdout)?;
        return Ok(());
    }

    if opts.context_prune || opts.cat_compress {
        let report = hooks_impl::registry::run_install(opts.global)?;
        let feature_report = hooks_impl::run_install_features(opts.context_prune, opts.cat_compress)?;

        writeln!(stdout, "{}", ui::logo())?;
        writeln!(stdout, "{}", ui::hr("install"))?;
        writeln!(stdout)?;
        writeln!(stdout, "{}", ui::box_top())?;
        for row in report {
            let (symbol, outcome_str) = format_outcome(&row.outcome);
            writeln!(stdout, "  {}  {} {:<22}  {}", symbol, outcome_str, row.target.bold(), row.path.dim())?;
        }
        writeln!(stdout, "{}", ui::box_bottom())?;
        writeln!(stdout)?;
        writeln!(stdout, "{}", ui::hr("features"))?;
        writeln!(stdout)?;
        if opts.context_prune {
            writeln!(stdout, "  {}  {} — {}", "🧠".to_string(), "Context pruning".bold(), "`shard distill` compacts conversation history".dim())?;
        }
        if opts.cat_compress {
            writeln!(stdout, "  {}  {} — {}", "📄".to_string(), "File-read compression".bold(), "`shard cat` compacts file reads for AI".dim())?;
        }
        writeln!(stdout)?;
        writeln!(stdout, "{}", ui::box_top())?;
        for row in feature_report {
            let (symbol, outcome_str) = format_outcome(&row.outcome);
            writeln!(stdout, "  {}  {} {:<22}  {}", symbol, outcome_str, row.target.bold(), row.path.dim())?;
        }
        writeln!(stdout, "{}", ui::box_bottom())?;
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  {}  {}",
            "▸".dim(),
            "Restart your shell (or run `source ~/.bashrc`) for aliases to take effect.".dim()
        )?;
        writeln!(
            stdout,
            "  {}  {}",
            "▸".dim(),
            "To disable features: run `shard init --uninstall`.".dim()
        )?;
        writeln!(stdout)?;
        return Ok(());
    }

    let report = hooks_impl::registry::run_install(opts.global)?;
    writeln!(stdout, "{}", ui::logo())?;
    writeln!(stdout, "{}", ui::hr("install"))?;
    writeln!(stdout)?;
    writeln!(stdout, "{}", ui::box_top())?;
    for row in report {
        let (symbol, outcome_str) = format_outcome(&row.outcome);
        writeln!(stdout, "  {}  {} {:<22}  {}", symbol, outcome_str, row.target.bold(), row.path.dim())?;
    }
    writeln!(stdout, "{}", ui::box_bottom())?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "  {}  {}",
        "▸".dim(),
        "Restart your shell (or run `source ~/.bashrc`) for aliases to take effect.".dim()
    )?;
    writeln!(stdout)?;
    Ok(())
}

fn format_outcome(outcome: &EditOutcome) -> (String, String) {
    match outcome {
        EditOutcome::Installed => ("🔗".to_string(), "Installed".green().to_string()),
        EditOutcome::Replaced => ("🔄".to_string(), "Updated".yellow().to_string()),
        EditOutcome::AlreadyPresent => ("●".to_string(), "Present".green().to_string()),
        EditOutcome::Removed => ("🗑".to_string(), "Removed".green().to_string()),
        EditOutcome::NotFound => ("○".to_string(), "Not found".dim().to_string()),
    }
}
