/// One memory that owes the reader an English search summary.
///
/// `faults` is empty when the memory has no summary at all, and names what
/// is wrong with the one it has otherwise; either way the text is here so
/// the writer that renders it does not have to fetch it again.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PendingSummary {
    pub about: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub faults: Vec<String>,
}
