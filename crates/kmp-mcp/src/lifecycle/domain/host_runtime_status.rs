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
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Connected | Self::Registered)
    }

    pub fn description(&self) -> String {
        match self {
            Self::Connected => "host reports the MCP connected".to_string(),
            Self::Registered => "host reports the MCP registered and enabled".to_string(),
            Self::Disabled => "the KMP MCP registration is disabled".to_string(),
            Self::Missing => "the host has no KMP MCP registration".to_string(),
            Self::PendingApproval => "the KMP MCP registration is pending approval".to_string(),
            Self::Failed(detail) => detail.clone(),
        }
    }
}
