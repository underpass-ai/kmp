/// How one proof object's canonical body was delivered.
///
/// `has_body` answers whether the store holds a body at all. It never answered
/// whether this response carried one, and once bodies can be withheld the two
/// questions stop having the same answer. This states the delivery, so no
/// caller has to infer it — and so that "a card exists" can never be mistaken
/// for "the canonical body is here".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceBodyState {
    /// The canonical text is in this response.
    Loaded,
    /// The store holds it, it was requested, and the byte ceiling was reached
    /// first. The object carries its descriptor and what it would cost.
    DeferredBudget,
    /// The store holds it and this named expansion did not ask for it.
    NotRequested,
    /// A reader-authored card stood for it, so the canonical text was not read.
    /// This is orientation, never proof.
    Compact,
    /// The store holds no body for this node.
    Missing,
}

impl TraceBodyState {
    pub const fn as_str(self) -> &'static str {
        match self {
            TraceBodyState::Loaded => "loaded",
            TraceBodyState::DeferredBudget => "deferred_budget",
            TraceBodyState::NotRequested => "not_requested",
            TraceBodyState::Compact => "compact",
            TraceBodyState::Missing => "missing",
        }
    }

    /// Whether this response withheld a body the store actually holds. These
    /// are the gaps that propagate from a source to every entry it supports,
    /// and that keep a group out of `complete_groups`.
    pub const fn is_omitted(self) -> bool {
        matches!(
            self,
            TraceBodyState::DeferredBudget | TraceBodyState::NotRequested | TraceBodyState::Compact
        )
    }
}
