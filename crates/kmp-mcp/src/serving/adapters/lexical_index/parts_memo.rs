use std::collections::HashMap;

use kmp_domain::{NodeDetailProjection, NodeProjection, NodeRelationProjection};

use super::node_state::NodeState;

/// What the parts of one ask read point by point, kept across the top-k
/// rounds of that ask (P14): every round reads the store at the same event
/// position and the sidecar at the position it followed to, so a node, its
/// kept edges, its body and its sidecar state read in an earlier round are
/// what a later round would read again. A round walks the whole selection
/// afresh over these answers, so its parts are exactly those a read from
/// nothing gives; only the repeated reads are saved.
///
/// It lives for one ask and one position; nothing outlives the ask.
#[derive(Debug, Default)]
pub(super) struct PartsMemo {
    pub(super) states: HashMap<String, Option<NodeState>>,
    pub(super) nodes: HashMap<String, Option<NodeProjection>>,
    pub(super) details: HashMap<String, Option<NodeDetailProjection>>,
    /// The kept edges asked for at a node, by the node, out of it, into it,
    /// and of one relation type when named.
    #[allow(clippy::type_complexity)]
    pub(super) edges:
        HashMap<(String, bool, bool, Option<&'static str>), Vec<NodeRelationProjection>>,
    /// The round reads every candidate left, so no round follows: what it
    /// reads is served from here when an earlier round read it, and is not
    /// kept, so the last round does not hold its parts twice.
    pub(super) read_through: bool,
    /// How many point reads went to the store or the sidecar, for
    /// the tests that pin what a round costs.
    #[cfg(test)]
    pub(super) misses: usize,
}
