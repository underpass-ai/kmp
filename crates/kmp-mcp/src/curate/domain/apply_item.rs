/// One item the agent accepts from a review, in its own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplyItem {
    pub item_id: String,
    pub why: String,
    pub evidence: String,
    pub confidence: Option<String>,
    pub rel: Option<String>,
    pub reverse: bool,
    pub confirm_doubted: bool,
}
