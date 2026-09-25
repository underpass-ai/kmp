//! The whole paths drawn on the loom.

use crate::view::domain::memory_ref::MemoryRef;
use crate::view::domain::path_chain::PathChain;
use crate::view::domain::path_step::PathStep;

/// The answer of one `kmp_curate` path search, kept as the view shows it.
///
/// Unlike a trace, the chains are stored rather than re-asked: a path search
/// may consult a paid judge, and a proposed step is only declarable through
/// the review it was frozen in. The view holds that review's token so the
/// loom can compose the declaring call; it never writes memory itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawnPaths {
    /// Where every chain starts.
    pub from: MemoryRef,
    /// Where the chains had to arrive, when a goal was named.
    pub to: Option<MemoryRef>,
    /// The longest chain the search considered, when the caller bounded it.
    pub max_hops: Option<u8>,
    /// The frozen review the proposed steps belong to.
    pub review_token: Option<String>,
    /// The search's own one-line account.
    pub summary: String,
    /// The chains, fewest proposed steps first.
    pub chains: Vec<PathChain>,
    /// Declared relations the walk left out because the judge found their
    /// why and evidence do not hold.
    pub avoided: Vec<PathStep>,
    /// What the search could not do, in its words.
    pub warnings: Vec<String>,
}

impl DrawnPaths {
    /// Every fact the chains and avoided declarations touch, once, in the
    /// order they are walked.
    pub fn refs(&self) -> Vec<&MemoryRef> {
        let mut refs: Vec<&MemoryRef> = Vec::new();
        let steps = self
            .chains
            .iter()
            .flat_map(|chain| &chain.steps)
            .chain(&self.avoided);
        for step in steps {
            for end in [&step.from.reference, &step.to.reference] {
                if !refs.contains(&end) {
                    refs.push(end);
                }
            }
        }
        refs
    }
}
