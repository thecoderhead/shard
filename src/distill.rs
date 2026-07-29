use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::compact;
use crate::intent::Intent;
use crate::tokens;
use crate::ui;

pub struct DistillOptions {
    pub file: Option<PathBuf>,
    pub intent: Option<Intent>,
    pub quiet: bool,
    pub window_lines: usize,
}

pub fn run(opts: DistillOptions) -> Result<()> {
    let input = read_input(&opts.file)?;

    if input.trim().is_empty() {
        return Ok(());
    }

    let sections = if opts.window_lines > 0 {
        let lines: Vec<&str> = input.lines().collect();
        lines
            .chunks(opts.window_lines)
            .map(|chunk| chunk.join("\n"))
            .collect()
    } else {
        split_paragraphs(&input)
    };

    let mut total_tokens_in = 0u64;
    let mut total_tokens_out = 0u64;

    for section in &sections {
        let trimmed = section.trim();
        if trimmed.is_empty() {
            continue;
        }
        let compacted = compact::compact(trimmed, opts.intent.as_ref());
        let tokens_in = tokens::approx(trimmed.as_bytes());
        let tokens_out = tokens::approx(compacted.text.as_bytes());
        total_tokens_in += tokens_in;
        total_tokens_out += tokens_out;
        print!("{}", compacted.text);
        if !compacted.text.ends_with('\n') {
            println!();
        }
        println!();
    }

    if !opts.quiet {
        let pct = tokens::savings_pct(total_tokens_in, total_tokens_out);
        let footer = ui::savings_footer("distill", total_tokens_in, total_tokens_out, pct, "context pruned");
        eprint!("{}", footer);
    }

    Ok(())
}

fn read_input(path: &Option<PathBuf>) -> Result<String> {
    match path {
        Some(p) => fs::read_to_string(p)
            .with_context(|| format!("failed to read {}", p.display())),
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read stdin")?;
            Ok(buf)
        }
    }
}

fn split_paragraphs(text: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if line.trim().is_empty() && !current.is_empty() {
            sections.push(std::mem::take(&mut current));
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        sections.push(current);
    }

    sections
}
