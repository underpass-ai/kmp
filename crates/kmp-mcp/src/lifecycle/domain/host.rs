use std::fmt;

use serde::Serialize;

/// Native host whose plugin consumes the local KMP engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Claude,
    Codex,
}

impl Host {
    pub const CONVERGENCE_ORDER: [Self; 2] = [Self::Claude, Self::Codex];

    pub fn executable(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn owns_plugin_engine(self) -> bool {
        matches!(self, Self::Claude)
    }
}

impl fmt::Display for Host {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        })
    }
}
