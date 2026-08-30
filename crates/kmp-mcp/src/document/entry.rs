/// One remembered entry, as the document shows it.
pub(super) struct Entry {
    pub(super) reference: String,
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) observed_at: Option<String>,
}
