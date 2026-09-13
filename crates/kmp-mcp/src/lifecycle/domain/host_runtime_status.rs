use super::host_connection_verification::HostConnectionVerification;

/// What the native host reports for the MCP registration it will actually use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostRuntimeStatus {
    Connected,
    Registered,
    Disabled,
    Missing,
    PendingApproval,
    Failed(String),
}

impl HostRuntimeStatus {
    /// What this report proves about a live connection.
    ///
    /// `Registered` is deliberately not a connection. A host manager's
    /// inventory knows that a registration exists and is enabled; it has not
    /// spoken MCP to anything. Calling that usable is what let #680 report a
    /// healthy Codex host while no conversation had any KMP tool.
    pub fn verification(&self) -> HostConnectionVerification {
        match self {
            Self::Connected => HostConnectionVerification::Verified,
            Self::Registered => HostConnectionVerification::Unverified,
            Self::Disabled | Self::Missing | Self::PendingApproval | Self::Failed(_) => {
                HostConnectionVerification::Broken
            }
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Connected => "host reports the MCP connected".to_string(),
            Self::Registered => {
                "host reports the MCP registered and enabled, which is an inventory entry rather \
                 than a connection: nothing has spoken MCP to this engine from here"
                    .to_string()
            }
            Self::Disabled => "the KMP MCP registration is disabled".to_string(),
            Self::Missing => "the host has no KMP MCP registration".to_string(),
            Self::PendingApproval => "the KMP MCP registration is pending approval".to_string(),
            Self::Failed(detail) => detail.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three states a reader has to be able to tell apart.
    #[test]
    fn a_registration_is_never_mistaken_for_a_connection() {
        assert_eq!(
            HostRuntimeStatus::Connected.verification(),
            HostConnectionVerification::Verified
        );
        assert_eq!(
            HostRuntimeStatus::Registered.verification(),
            HostConnectionVerification::Unverified
        );
        for broken in [
            HostRuntimeStatus::Disabled,
            HostRuntimeStatus::Missing,
            HostRuntimeStatus::PendingApproval,
            HostRuntimeStatus::Failed("CONNECTION_CLOSED".to_string()),
        ] {
            assert_eq!(
                broken.verification(),
                HostConnectionVerification::Broken,
                "{broken:?}"
            );
        }
    }

    #[test]
    fn a_registration_says_what_it_is_not_evidence_of() {
        let described = HostRuntimeStatus::Registered.description();
        assert!(described.contains("registered and enabled"), "{described}");
        assert!(
            described.contains("rather than a connection"),
            "{described}"
        );
        assert_eq!(
            HostRuntimeStatus::Connected.description(),
            "host reports the MCP connected"
        );
    }
}
