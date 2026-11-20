use super::host::Host;
use super::host_installation::HostInstallation;

/// What every host's plugin manager reported, one host at a time.
///
/// One host whose inventory cannot be read is that host's problem. It used
/// to fail the whole inventory, so a Hermes on PATH with no KMP mapping
/// stopped setup, update and doctor for Claude Code and Codex too (S11).
/// The hosts that answered are kept; the ones that did not are named with
/// the reason, for the caller to report against that host alone.
#[derive(Clone, Debug, Default)]
pub struct HostInventory {
    installed: Vec<HostInstallation>,
    unreadable: Vec<(Host, String)>,
}

impl HostInventory {
    pub fn new(installed: Vec<HostInstallation>, unreadable: Vec<(Host, String)>) -> Self {
        Self {
            installed,
            unreadable,
        }
    }

    /// An inventory in which every host answered.
    pub fn of(installed: Vec<HostInstallation>) -> Self {
        Self::new(installed, Vec::new())
    }

    pub fn installed(&self) -> &[HostInstallation] {
        &self.installed
    }

    /// Hosts whose inventory could not be read, each with why.
    pub fn unreadable(&self) -> &[(Host, String)] {
        &self.unreadable
    }

    /// Why `host`'s inventory could not be read, when it could not.
    pub fn unreadable_reason(&self, host: Host) -> Option<&str> {
        self.unreadable
            .iter()
            .find(|(candidate, _)| *candidate == host)
            .map(|(_, reason)| reason.as_str())
    }
}
