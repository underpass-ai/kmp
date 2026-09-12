/// What a stored card declares about itself, without its prose.
///
/// A read that may not show the text still owes the reader the provenance:
/// which body version the card was authored from, which card revision it is,
/// who wrote it and when. Separating the stamp from the text is what makes a
/// stale or post-cut card structurally unable to leak its prose — there is no
/// text field on this type to forget to clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCardStamp {
    pub source_revision: u64,
    pub source_content_hash: String,
    pub source_record_digest: String,
    pub source_body_bytes: u64,
    pub card_revision: u64,
    pub authored_by: String,
    pub authored_at: String,
    /// Byte length of the stored card text. Reported so a reader can see what
    /// a regeneration would buy without being shown the text itself.
    pub text_bytes: u64,
}
