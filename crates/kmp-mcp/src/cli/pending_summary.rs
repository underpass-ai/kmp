use kmp_mcp::summaries::AuditedSummary;

/// One memory that owes the reader an English search summary, in the shape
/// `summaries pending --json` prints it.
///
/// This is the terminal's own wire format and lives at the terminal's
/// boundary: the shared reading returns [`AuditedSummary`] values and decides
/// nothing about how a surface renders them, exactly as the MCP tool maps
/// the same values into its own response shape.
///
/// `faults` is empty when the memory has no summary at all, and names what
/// is wrong with the one it has otherwise; either way the text is here so
/// the writer that renders it does not have to fetch it again.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct PendingSummary {
    pub(super) about: String,
    #[serde(rename = "ref")]
    pub(super) reference: String,
    pub(super) kind: String,
    pub(super) text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) faults: Vec<String>,
}

impl PendingSummary {
    /// The debt one audited memory carries. A memory that owes nothing is
    /// never rendered this way; the verb filters before it maps.
    pub(super) fn of(audited: &AuditedSummary) -> Self {
        Self {
            about: audited.about.clone(),
            reference: audited.reference.clone(),
            kind: audited.kind.clone(),
            text: audited.text.clone().unwrap_or_default(),
            faults: audited.fault_sentences(),
        }
    }
}
