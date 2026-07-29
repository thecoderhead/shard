use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::compact;
use crate::intent::Intent;
use crate::tokens;
use crate::ui;

pub struct CatOptions {
    pub path: PathBuf,
    pub intent: Option<Intent>,
    pub quiet: bool,
    pub head_lines: Option<usize>,
}

pub fn run(opts: CatOptions) -> Result<()> {
    let mut content = fs::read_to_string(&opts.path)
        .with_context(|| format!("failed to read {}", opts.path.display()))?;

    if let Some(n) = opts.head_lines {
        let head: String = content.lines().take(n).collect::<Vec<_>>().join("\n");
        content = head;
    }

    if content.trim().is_empty() {
        return Ok(());
    }

    let tokens_in = tokens::approx(content.as_bytes());
    let compacted = compact::compact(&content, opts.intent.as_ref());
    let tokens_out = tokens::approx(compacted.text.as_bytes());

    print!("{}", compacted.text);
    if !compacted.text.ends_with('\n') {
        println!();
    }

    if !opts.quiet {
        let pct = tokens::savings_pct(tokens_in, tokens_out);
        let extra = format!("{}  ↙ {}", opts.path.display(), compacted.archetype.as_str());
        let footer = ui::savings_footer("cat", tokens_in, tokens_out, pct, &extra);
        eprint!("{}", footer);
    }

    Ok(())
}
