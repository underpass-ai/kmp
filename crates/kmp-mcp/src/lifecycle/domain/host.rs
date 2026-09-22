use std::fmt;

use serde::Serialize;

/// Native host whose plugin consumes the local KMP engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Claude,
    Codex,
    Hermes,
}

impl Host {
    pub const CONVERGENCE_ORDER: [Self; 3] = [Self::Claude, Self::Codex, Self::Hermes];

    pub fn executable(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Hermes => "hermes",
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
            Self::Hermes => "hermes",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_is_a_peer_in_convergence_order_after_the_plugin_hosts() {
        assert_eq!(
            Host::CONVERGENCE_ORDER,
            [Host::Claude, Host::Codex, Host::Hermes]
        );
    }

    #[test]
    fn hermes_declares_its_own_executable_name() {
        assert_eq!(Host::Hermes.executable(), "hermes");
        assert_eq!(Host::Hermes.to_string(), "hermes");
    }

    #[test]
    fn only_claude_owns_a_plugin_engine() {
        assert!(Host::Claude.owns_plugin_engine());
        assert!(!Host::Codex.owns_plugin_engine());
        assert!(!Host::Hermes.owns_plugin_engine());
    }
}
