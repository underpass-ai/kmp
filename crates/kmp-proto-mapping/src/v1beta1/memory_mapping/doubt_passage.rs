/// One admitted memory a doubt band sends to a judge: its evidence id, the
/// entry it belongs to, its exact text and that text's SHA-256, whether the deterministic answer
/// cites it, and whether it may be promoted into the core (it passed the
/// anchored gate, or no anchor was required). The judge answers about the
/// identity (id, text digest); nothing it returns is text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubtPassage {
    pub id: String,
    pub entry_ref: String,
    pub text: String,
    pub text_sha256: String,
    pub in_core: bool,
    pub promotable: bool,
}
