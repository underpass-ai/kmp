/// A link's storage keys and projected endpoint records. A missing record
/// stays absent; a JSON field missing from an existing record stays null.
pub(crate) struct LinkedJsonRow {
    pub source: String,
    pub target: String,
    pub source_json: Option<Vec<u8>>,
    pub target_json: Option<Vec<u8>>,
}
