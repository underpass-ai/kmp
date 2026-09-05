use serde::Serialize;

/// What a lifecycle run did about the lexical-bridge table, at the CLI
/// boundary. `outcome` is the machine-readable word; `detail` is the line a
/// human reads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleBridgeDto {
    pub outcome: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// The digest of the table this run replaced, when the machine held a
    /// different one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_sha256: Option<String>,
    /// Whether `ask` can cross languages on this machine after the run.
    pub crosses_languages: bool,
}
