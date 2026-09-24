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

    /// Whether this host installs the marketplace plugin tree whose bytes must
    /// be identical across hosts. Hermes ships skills and an MCP registration
    /// into its own home instead, so it takes no part in tree parity (#849).
    pub fn installs_plugin_tree(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
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

    #[test]
    fn only_the_marketplace_hosts_install_a_plugin_tree() {
        assert!(Host::Claude.installs_plugin_tree());
        assert!(Host::Codex.installs_plugin_tree());
        assert!(!Host::Hermes.installs_plugin_tree());
    }
}
