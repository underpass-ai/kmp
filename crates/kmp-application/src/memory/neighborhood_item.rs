use std::collections::BTreeMap;

/// Literal material for a writer to review, never a generated conclusion.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NeighborhoodItem {
    pub about: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub state: String,
    pub kind: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub text_omitted: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub clocks: BTreeMap<String, Vec<String>>,
}
