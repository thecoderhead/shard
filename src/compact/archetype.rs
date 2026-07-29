//! Archetype enum shared by classifier and engine.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Archetype {
    Tabular,
    LinearLog,
    Tree,
    Passthrough,
}

impl Archetype {
    pub fn as_str(&self) -> &'static str {
        match self {
            Archetype::Tabular => "tabular",
            Archetype::LinearLog => "linear-log",
            Archetype::Tree => "tree",
            Archetype::Passthrough => "passthrough",
        }
    }
}
