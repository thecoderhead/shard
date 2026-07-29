use crate::compact::Archetype;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    JsonAnnotated,
}

impl OutputFormat {
    pub fn from_env() -> Self {
        match std::env::var("SHARD_OUTPUT_FORMAT").as_deref() {
            Ok("json-annotated") => Self::JsonAnnotated,
            _ => Self::Text,
        }
    }
}

pub fn format_footer(
    format: OutputFormat,
    tokens_in: u64,
    tokens_out: u64,
    savings_pct: f64,
    archetype: Archetype,
    log_path: &std::path::Path,
) -> String {
    match format {
        OutputFormat::Text => {
            let extra = format!("↙ log: {}", log_path.display());
            ui::savings_footer("shard", tokens_in, tokens_out, savings_pct, &extra)
        }
        OutputFormat::JsonAnnotated => {
            let json = serde_json::json!({
                "shard": {
                    "version": env!("CARGO_PKG_VERSION"),
                    "tokens_in": tokens_in,
                    "tokens_out": tokens_out,
                    "tokens_saved": tokens_in.saturating_sub(tokens_out),
                    "savings_pct": format!("{:.1}", savings_pct),
                    "archetype": archetype.as_str(),
                    "log_path": log_path.display().to_string(),
                    "recover_full_output": {
                        "command": "cat",
                        "args": [log_path.display().to_string()]
                    }
                }
            });
            format!("{}\n", serde_json::to_string(&json).unwrap_or_default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_format_contains_expected_keys() {
        let output = format_footer(
            OutputFormat::JsonAnnotated,
            1200, 200, 83.3,
            Archetype::LinearLog,
            std::path::Path::new(".shard/logs/test.log"),
        );
        assert!(output.contains("\"tokens_in\":1200"));
        assert!(output.contains("\"tokens_out\":200"));
        assert!(output.contains("\"linear-log\""));
        assert!(output.contains("recover_full_output"));
    }

    #[test]
    fn text_format_contains_visual_elements() {
        let output = format_footer(
            OutputFormat::Text,
            1200, 200, 83.3,
            Archetype::LinearLog,
            std::path::Path::new(".shard/logs/test.log"),
        );
        assert!(output.contains("◈"));
        assert!(output.contains("shard"));
    }
}
