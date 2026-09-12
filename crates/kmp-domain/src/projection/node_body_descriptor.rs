/// Everything a read needs to know about one canonical body without loading it.
///
/// The public `content_hash` cannot carry this weight alone. It is derived from
/// the event and the node identity, and a projection writer accepts a detail
/// directly, so a write can change the stored text without a reliable change to
/// that token. `record_digest` is computed over the exact stored record bytes at
/// write time and is the identity a card or an expansion may bind to.
///
/// Storage owns this. It is not a reader's claim and not a field of a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBodyDescriptor {
    pub node_id: String,
    pub revision: u64,
    /// The public token, copied verbatim. Never reinterpreted as a body digest.
    pub content_hash: String,
    /// Bytes of the record as stored, which is what a byte ceiling spends.
    pub record_bytes: u64,
    /// Canonical UTF-8 bytes of the body text, which is what `body_bytes`
    /// counts once a body is actually loaded. Escaping makes the two differ.
    pub body_bytes: u64,
    /// `sha256:<lowercase hex>` over the exact stored record bytes.
    pub record_digest: String,
}
