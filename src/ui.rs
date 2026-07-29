//! Shared UI utilities for Shard's CLI — cyberpunk/HUD visual language.
//!
//! Every subcommand draws from the same palette: matrix-cyan data streams,
//! neon-magenta glitch accents, HUD box-frames with signal meters, and
//! terminal-scan animations.

use crossterm::style::{Color, Stylize};

// ── Palette ─────────────────────────────────────────────────────────────────
//   CYAN     primary (frames, headers, data rails)
//   MAGENTA  glitch accents, critical highlights
//   GREEN    success, high efficiency, signal lock
//   YELLOW   warning, medium efficiency
//   RED      error, low efficiency, danger
//   DIM      secondary metadata, scanlines

const P: &str = "  ";

fn tag() -> String {
    format!("{}◆ SHARD ◆{}", "╭".cyan().bold(), "╮".cyan().bold())
}

/// ASCII HUD logo with version tag and scanline.
pub fn logo() -> String {
    let c = Color::Cyan;
    let m = Color::Magenta;
    format!(
        r#" {c}  ███████  ██  ██   ██████   ██████  ██████
 {c} ██       ██  ██  ██  ██  ██  ██  ██  ██
 {m} ███████  ██  ██  ██████  ██████  ██  ██
 {m}  ███████  ██  ██  ██  ██  ██  ██  ██████{reset}   {ver}
{scan}
{nfo}",
        c = c.to_string().bold(),
        m = m.bold(),
        reset = Color::Reset.to_string(),
        ver = glitch_label(&format!("v{}", env!("CARGO_PKG_VERSION")), false),
        scan = scanline(),
        nfo = topo_tag(),
    )
}

fn topo_tag() -> String {
    format!(" {}  SYS::NODE  PROTOCOL::CLI  ENCODING::UTF-8", P)
        .magenta()
        .dim()
        .to_string()
}

/// Compact glitch logo for footers.
pub fn logo_compact() -> String {
    tag()
}

/// Scanline divider — a full row of light/dark blocks.
pub fn scanline() -> String {
    format!("{}", "▔".repeat(76).dim())
}

/// A second scanline variant (alternating).
fn scanline_dots() -> String {
    format!("{}", "· ".repeat(38).dim())
}

/// HUD-style horizontal rule with centered uppercase label.
pub fn hr(label: &str) -> String {
    let width = 72usize;
    if label.is_empty() {
        return format!("{}<{:─^width$}>", P, "", width = width.saturating_sub(4)).dim().to_string();
    }
    let padded = format!(" {} ", label.to_uppercase());
    let left = (width.saturating_sub(padded.len())) / 2;
    let right = width - left - padded.len();
    let line_l = "═".repeat(left);
    let line_r = "═".repeat(right);
    format!(
        "{}╞{}{}{}╡",
        P,
        line_l.dim(),
        padded.cyan().bold(),
        line_r.dim(),
    )
}

/// HUD-style box with glitch corners.
pub fn box_top() -> String {
    format!(" {}┌{}┐{}", "".to_string(), "─".repeat(62).cyan().dim(), glitch())
}
pub fn box_bottom() -> String {
    format!(" {}└{}┘{}", "".to_string(), "─".repeat(62).cyan().dim(), glitch())
}

/// Glitch text effect: wraps a label with inverted-bracket noise.
fn glitch() -> String {
    // Just a small spark symbol for terminal flavour — static per line is enough.
    " ⚡".magenta().dim().to_string()
}

fn glitch_label(text: &str, intense: bool) -> String {
    let l = if intense { "⫷".magenta().bold().to_string() } else { "⫷".magenta().dim().to_string() };
    let r = if intense { "⫸".magenta().bold().to_string() } else { "⫸".magenta().dim().to_string() };
    let t = if intense {
        text.to_uppercase().white().bold().to_string()
    } else {
        text.cyan().to_string()
    };
    format!("{}{}{}", l, t, r)
}

/// Signal-strength meter: 5 bars like a phone / wi-fi indicator.
/// fraction 0.0–1.0
pub fn signal_meter(fraction: f64) -> String {
    let bars = 5;
    let filled = ((fraction * bars as f64).round() as usize).min(bars);
    let out: String = (0..bars)
        .map(|i| {
            if i < filled {
                let c = if fraction >= 0.8 { "█" } else if fraction >= 0.4 { "▓" } else { "▒" };
                c.green().bold().to_string()
            } else {
                "░".dim().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("");
    out
}

/// Generic key:value data row with a HUD prefix.
pub fn data_row(key: &str, value: &str) -> String {
    format!(
        "{}{} {:<22} {}",
        P,
        "┊".cyan().dim(),
        format!("{}:", key).bold(),
        value,
    )
}
pub fn data_row_green(key: &str, value: &str) -> String {
    format!(
        "{}{} {:<22} {}",
        P,
        "┊".cyan().dim(),
        format!("{}:", key).bold(),
        value.green().bold(),
    )
}
pub fn data_row_warn(key: &str, value: &str) -> String {
    format!(
        "{}{} {:<22} {}",
        P,
        "┊".cyan().dim(),
        format!("{}:", key).bold(),
        value.yellow().bold(),
    )
}

/// Status indicator with a live/blink-style dot.
pub fn status_node(label: &str, ok: bool, detail: &str) -> String {
    let mark = if ok {
        "● LIVE".green().bold().to_string()
    } else {
        "● DOWN".red().bold().to_string()
    };
    format!("{}{} {:<12}  {:<16} {}", P, mark, label.bold(), detail.dim(), "")
}

/// Simple ok/fail rows.
pub fn ok(label: &str) -> String {
    format!("{}{}  {}  {}", P, "●".green().bold(), "LOCK".green().bold(), label)
}
pub fn fail(label: &str, detail: &str) -> String {
    format!("{}{}  {}  {}  {}", P, "●".red().bold(), "ALERT".red().bold(), label, detail.red())
}

/// Progress bar with hex-gauge style and readout.
pub fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    // Paint the filled section with a colour gradient.
    let gradient: String = (0..filled)
        .map(|i| {
            let pos = i as f64 / width.max(1) as f64;
            let block = if fraction >= 0.95 { "█" } else if i % 3 == 0 { "▓" } else { "█" };
            match pos {
                _ if pos > 0.85 => block.cyan().bold().to_string(),
                _ if pos > 0.55 => block.green().bold().to_string(),
                _ if pos > 0.25 => block.yellow().bold().to_string(),
                _ => block.red().bold().to_string(),
            }
        })
        .collect::<String>();
    let empty_bar: String = std::iter::repeat('░')
        .take(empty)
        .collect::<String>()
        .dim();
    let pct = format!("{:5.1}%", fraction * 100.0);
    format!("{}{} [{}]", gradient, empty_bar, pct.bold())
}

/// A hex-dump style row: shows hex offset + mini byte visualization.
pub fn hex_gauge(label: &str, value: u64, max: u64, unit: &str) -> String {
    let fraction = if max > 0 { value as f64 / max as f64 } else { 0.0 };
    let bar = progress_bar(fraction, 30);
    let hex_val = format!("0x{:X}", value);
    format!(
        "{}{} {:<18} {}  {} {}",
        P,
        "◈".cyan().bold(),
        format!("{}:", label).bold(),
        hex_val.cyan().dim(),
        bar,
        unit.dim(),
    )
}

/// Savings footer — used by distill, cat, and PTY bridge.
pub fn savings_footer(
    label: &str,
    tokens_in: u64,
    tokens_out: u64,
    savings_pct: f64,
    extra: &str,
) -> String {
    let bar = progress_bar(savings_pct / 100.0, 14);
    let saved = tokens_in.saturating_sub(tokens_out);
    let sig = signal_meter(savings_pct / 100.0);
    format!(
        "{}{} │ {} → {}  {}  {}  {}  {}  {}\n",
        P,
        glitch_label(label, false),
        tokens_in.to_string().bold(),
        tokens_out.to_string().green().bold(),
        format!("{:.1}%", savings_pct).green().bold(),
        saved.to_string().bold(),
        bar,
        sig,
        extra.dim(),
    )
}

/// Token histogram: visual bars for tokens-in vs tokens-out.
pub fn token_histogram(tokens_in: u64, tokens_out: u64, width: usize) -> String {
    let max = tokens_in.max(tokens_out).max(1);
    let in_w = ((tokens_in as f64 / max as f64) * width as f64).round() as usize;
    let out_w = ((tokens_out as f64 / max as f64) * width as f64).round() as usize;
    let in_bar: String = std::iter::repeat('█').take(in_w.min(width)).collect();
    let out_bar: String = std::iter::repeat('█').take(out_w.min(width)).collect();
    format!(
        "{} IN:  {} {:>8}\n{} OUT: {} {:>8}",
        P,
        in_bar.cyan().bold(),
        tokens_in.to_string().bold(),
        P,
        out_bar.green().bold(),
        tokens_out.to_string().bold(),
    )
}

/// Stats divider line.
pub fn divider() -> String {
    format!("{}{}", P, "┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈".dim())
}

pub fn section_header(label: &str) -> String {
    format!(" {} {} {} ", "│".cyan().bold(), label.cyan().bold(), "│".cyan().bold())
}

pub fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

pub fn fmt_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    } else {
        format!("{}h {}m", ms / 3_600_000, (ms % 3_600_000) / 60_000)
    }
}
