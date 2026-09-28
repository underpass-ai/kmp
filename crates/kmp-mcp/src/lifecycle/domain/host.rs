use std::fmt;

use serde::Serialize;

/// Native host whose plugin consumes the local KMP engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Claude,
    Codex,
    Hermes,
    Pi,
}

impl Host {
    pub const CONVERGENCE_ORDER: [Self; 4] = [Self::Claude, Self::Codex, Self::Hermes, Self::Pi];

    pub fn executable(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Hermes => "hermes",
            Self::Pi => "pi",
        }
    }

    pub fn owns_plugin_engine(self) -> bool {
        matches!(self, Self::Claude)
    }

    /// Whether this host installs the marketplace plugin tree whose bytes must
    /// be identical across hosts. Hermes ships skills and an MCP registration
    /// into its own home instead, so it takes no part in tree parity (#849);
    /// neither does Pi, whose skills live in its agent home and whose MCP
    /// connection comes from the `underpass-pi` package.
    pub fn installs_plugin_tree(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }

    /// The package, installed by something other than KMP, that carries this
    /// host's MCP connection. Pi has no native MCP: `underpass setup` installs
    /// `underpass-pi`, and KMP can only observe it, never provide it.
    pub fn external_package(self) -> Option<&'static str> {
        match self {
            Self::Pi => Some("underpass-pi"),
            Self::Claude | Self::Codex | Self::Hermes => None,
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Hermes => "hermes",
            Self::Pi => "pi",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_is_a_peer_in_convergence_order_after_the_plugin_hosts() {
        assert_eq!(
            Host::CONVERGENCE_ORDER[..3],
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
    fn pi_is_the_last_peer_in_convergence_order() {
        assert_eq!(
            Host::CONVERGENCE_ORDER,
            [Host::Claude, Host::Codex, Host::Hermes, Host::Pi]
        );
    }

    #[test]
    fn pi_declares_its_own_executable_name() {
        assert_eq!(Host::Pi.executable(), "pi");
        assert_eq!(Host::Pi.to_string(), "pi");
    }

    #[test]
    fn pi_neither_owns_a_plugin_engine_nor_installs_a_plugin_tree() {
        assert!(!Host::Pi.owns_plugin_engine());
        assert!(!Host::Pi.installs_plugin_tree());
    }

    #[test]
    fn only_pi_depends_on_a_package_kmp_does_not_install() {
        assert_eq!(Host::Pi.external_package(), Some("underpass-pi"));
        for host in [Host::Claude, Host::Codex, Host::Hermes] {
            assert_eq!(host.external_package(), None, "{host}");
        }
    }

    #[test]
    fn only_the_marketplace_hosts_install_a_plugin_tree() {
        assert!(Host::Claude.installs_plugin_tree());
        assert!(Host::Codex.installs_plugin_tree());
        assert!(!Host::Hermes.installs_plugin_tree());
    }
}
