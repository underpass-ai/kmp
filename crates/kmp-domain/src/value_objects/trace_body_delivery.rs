/// What this response did with the bodies of the selection it chose.
///
/// Reported so a caller can tell a complete delivery from a partial one
/// without counting objects, and so the two next-step minimums are never
/// confused. `rerun_record_bytes` is what the same whole query would need to
/// admit one more record: it pays for the admitted prefix again.
/// `named_record_bytes` is what a named expansion of that one record needs,
/// which is the record itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceBodyDelivery {
    pub loaded: u32,
    pub deferred_budget: u32,
    pub not_requested: u32,
    pub compact: u32,
    pub missing: u32,
    /// Stored record bytes admitted by this response.
    pub admitted_record_bytes: u64,
    /// Descriptor total of every present body in the selection, delivered or
    /// not. Never confused with `body_bytes`, which counts text actually read.
    pub selected_body_bytes: u64,
    /// The first record the ceiling refused, if any.
    pub next_deferred_ref: Option<String>,
    pub rerun_record_bytes: Option<u64>,
    pub named_record_bytes: Option<u64>,
}

impl TraceBodyDelivery {
    /// Whether the store holds a body this response did not carry.
    pub const fn is_partial(&self) -> bool {
        self.deferred_budget > 0 || self.not_requested > 0 || self.compact > 0
    }
}
