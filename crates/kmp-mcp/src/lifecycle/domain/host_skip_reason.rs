use super::host::Host;
use super::host_runtime_status::HostRuntimeStatus;

/// Why auto-detection left a host alone.
///
/// Only a host whose MCP connection comes from a package KMP cannot install
/// (Pi's `underpass-pi`) is ever skipped, and only when that package is not a
/// usable registration. Each reason names what the operator can do, and never
/// echoes the host's own configuration back.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostSkipReason {
    PackageMissing { host: Host, package: &'static str },
    ExtensionExcluded { host: Host, package: &'static str },
    SettingsUnreadable { host: Host },
}

impl HostSkipReason {
    /// Why an auto-detected host should be skipped, or none when it can be
    /// converged: a host KMP provisions itself, or a registered package.
    pub fn for_status(host: Host, status: &HostRuntimeStatus) -> Option<Self> {
        let package = host.external_package()?;
        match status {
            HostRuntimeStatus::Registered | HostRuntimeStatus::Connected => None,
            HostRuntimeStatus::Disabled => Some(Self::ExtensionExcluded { host, package }),
            HostRuntimeStatus::Failed(_) => Some(Self::SettingsUnreadable { host }),
            HostRuntimeStatus::Missing | HostRuntimeStatus::PendingApproval => {
                Some(Self::PackageMissing { host, package })
            }
        }
    }

    pub fn host(&self) -> Host {
        match self {
            Self::PackageMissing { host, .. }
            | Self::ExtensionExcluded { host, .. }
            | Self::SettingsUnreadable { host } => *host,
        }
    }

    pub fn warning(&self) -> String {
        match self {
            Self::PackageMissing { host, package } => {
                format!("{host} present but {package} not registered; run `underpass setup`")
            }
            Self::ExtensionExcluded { package, .. } => {
                format!("{package} registered but its KMP extension is excluded")
            }
            Self::SettingsUnreadable { host } => format!("{host} settings.json unreadable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warning(status: HostRuntimeStatus) -> Option<String> {
        HostSkipReason::for_status(Host::Pi, &status).map(|reason| reason.warning())
    }

    #[test]
    fn each_unusable_status_has_its_own_warning() {
        assert_eq!(
            warning(HostRuntimeStatus::Missing).as_deref(),
            Some("pi present but underpass-pi not registered; run `underpass setup`")
        );
        assert_eq!(
            warning(HostRuntimeStatus::Disabled).as_deref(),
            Some("underpass-pi registered but its KMP extension is excluded")
        );
        assert_eq!(
            warning(HostRuntimeStatus::Failed(
                "Pi settings.json is not valid JSON: {\"secret\": 1".to_string()
            ))
            .as_deref(),
            Some("pi settings.json unreadable"),
            "the warning never echoes what the settings file held"
        );
    }

    #[test]
    fn a_registered_package_is_never_skipped() {
        assert_eq!(warning(HostRuntimeStatus::Registered), None);
        assert_eq!(warning(HostRuntimeStatus::Connected), None);
    }

    #[test]
    fn hosts_kmp_provisions_itself_are_never_skipped() {
        for host in [Host::Claude, Host::Codex, Host::Hermes] {
            assert_eq!(
                HostSkipReason::for_status(host, &HostRuntimeStatus::Missing),
                None,
                "{host}"
            );
        }
    }

    #[test]
    fn a_reason_names_its_host() {
        let reason =
            HostSkipReason::for_status(Host::Pi, &HostRuntimeStatus::Disabled).expect("skipped");
        assert_eq!(reason.host(), Host::Pi);
    }
}
