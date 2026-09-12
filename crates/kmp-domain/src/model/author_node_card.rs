use crate::NodeCardExpectation;

/// One reader's request to author or refresh the compact card of one node
/// body, in one language.
///
/// The command names the body version it was written from. That declaration
/// is the whole contract: the kernel refuses the write if the store moved
/// underneath the reader, so an accepted card always describes a body version
/// that actually existed and that the card can be checked against later.
///
/// `authored_at` is stamped by the kernel before this command is built, never
/// taken from the caller: a backdated card would be invisible to a historical
/// read's cut and would leak later text into an earlier answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorNodeCard {
    pub about: String,
    pub node_id: String,
    pub language: String,
    pub text: String,
    pub source_revision: u64,
    pub source_content_hash: String,
    /// The record digest the reader read. Validity is decided on this.
    pub source_record_digest: String,
    pub expect: NodeCardExpectation,
    pub authored_by: String,
    pub authored_at: String,
}
