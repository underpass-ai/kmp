use std::collections::BTreeMap;

use kmp_domain::{GraphReadRevision, NodeDetailProjection, NodeNeighborhood};

/// The part of an about an ask read point by point instead of walking it
/// (DESIGN L6, P13): the root, the nodes and relations of the candidates the
/// lexical index reached and of the neighbourhood the ranker walks from
/// them, their bodies by node, and the revision they were read at.
#[derive(Debug, Clone)]
pub struct RecallParts {
    pub neighborhood: NodeNeighborhood,
    pub details: BTreeMap<String, NodeDetailProjection>,
    pub read_revision: Option<GraphReadRevision>,
}
