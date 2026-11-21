use std::path::PathBuf;

use super::host::Host;

/// The server id a plugin registers, and the one a hand-written block names.
pub const KMP_SERVER_ID: &str = "kmp";

/// The server id KMP used before it became `kmp`; nothing installs it now.
pub const RETIRED_SERVER_ID: &str = "kernel-memory";

/// An MCP registration of KMP written into a host's own configuration file
/// rather than installed by the plugin: `claude mcp add kmp`, or an
/// `[mcp_servers.kmp]` table in Codex's `config.toml`.
///
/// `uninstall` names it and `doctor` judges it from this one fact, so the two
/// can never disagree about what is wired on a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandWiredRegistration {
    host: Host,
    server_id: &'static str,
    path: PathBuf,
}

impl HandWiredRegistration {
    pub fn new(host: Host, server_id: &'static str, path: PathBuf) -> Self {
        Self {
            host,
            server_id,
            path,
        }
    }

    pub fn host(&self) -> Host {
        self.host
    }

    pub fn server_id(&self) -> &'static str {
        self.server_id
    }

    /// The configuration file that holds it.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Whether it names the retired id: that server is always a second one.
    pub fn is_retired(&self) -> bool {
        self.server_id == RETIRED_SERVER_ID
    }

    /// The edit that removes it. KMP never makes it: the file is not ours.
    pub fn removal(&self) -> String {
        match self.host {
            Host::Codex => format!("delete the [mcp_servers.{}] block", self.server_id),
            Host::Claude | Host::Hermes | Host::Pi => format!(
                "remove with:  {} mcp remove {}",
                self.host.executable(),
                self.server_id
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_host_is_told_the_edit_its_own_file_takes() {
        let codex = HandWiredRegistration::new(Host::Codex, KMP_SERVER_ID, "c.toml".into());
        assert_eq!(codex.removal(), "delete the [mcp_servers.kmp] block");
        assert!(!codex.is_retired());
        let claude = HandWiredRegistration::new(Host::Claude, RETIRED_SERVER_ID, "c.json".into());
        assert_eq!(
            claude.removal(),
            "remove with:  claude mcp remove kernel-memory"
        );
        assert!(claude.is_retired());
        assert_eq!(claude.host(), Host::Claude);
        assert_eq!(claude.server_id(), "kernel-memory");
        assert_eq!(claude.path(), std::path::Path::new("c.json"));
    }
}
