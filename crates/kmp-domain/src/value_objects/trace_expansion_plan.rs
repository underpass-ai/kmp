/// The next batch of bodies to ask for, decided over the complete proof
/// table before anything is paginated.
///
/// This exists because the projection that used to decide it could only see
/// one page. A response whose page happened to hold no pending ref offered
/// nothing, and the refs on the other pages were never named by any action:
/// following every action the protocol emitted still lost bodies. Which
/// bodies remain is a fact about the selection, not about how a response was
/// sliced, so the selection is where it is decided.
///
/// It carries no text, no descriptor and no state — only the refs of one
/// batch, what it costs, and the one oversized record that batch could not
/// take. A transport turns it into a call; nothing downstream re-derives
/// progress from a partial page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceExpansionPlan {
    /// Refs of the next batch, in manifest order, never more than
    /// `MAX_EXPANSION_REFS`. Empty when only an oversized record remains.
    pub refs: Vec<String>,
    /// The allowance that batch needs: the ceiling the caller chose when it
    /// set one, otherwise the exact sum of these records.
    pub record_bytes: u64,
    /// The first record of the remaining suffix that the caller's ceiling
    /// cannot hold on its own, with its exact size. It stays out of the batch
    /// and is offered separately, so the rest stays recoverable and no budget
    /// is raised behind the caller's back.
    pub oversized: Option<(String, u64)>,
}

impl TraceExpansionPlan {
    /// Whether this plan offers anything at all.
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty() && self.oversized.is_none()
    }
}
