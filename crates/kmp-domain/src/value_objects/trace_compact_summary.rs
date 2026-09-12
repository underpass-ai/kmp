/// What a compact presentation actually did to one trace response.
///
/// Reported because a shorter card does not by itself make a read cheaper:
/// the envelope, the stamps and the expand actions all cost bytes. These two
/// totals let a caller compare what the cards rendered against the canonical
/// bodies they stood in for, on this response, rather than trust a slogan.
///
/// The counts are presentation states, never proof completeness. A path whose
/// every node is `absent` is exactly as complete as it was without cards.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceCompactSummary {
    pub language: String,
    pub valid: u32,
    pub stale: u32,
    pub absent: u32,
    pub after_cut: u32,
    /// Bytes of card text this response renders.
    pub card_bytes: u64,
    /// Canonical body bytes this response did not render because a card stood
    /// for them. Bodies of nodes without a usable card are not counted here.
    pub body_bytes_omitted: u64,
}
