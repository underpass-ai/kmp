/// One typed link with the why and proof it was written with.
pub(super) struct Relation {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) rel: String,
    pub(super) why: Option<String>,
    pub(super) evidence: Option<String>,
}
