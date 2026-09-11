use crate::{NodeRelationProjection, TraceProofObject, TraceSearchStop};

/// Completeness of the fetched declared proof, never semantic answer sufficiency.
/// Objects and support rows are shared tables, not repeated inside each route.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceProofResult {
    pub objects: Vec<TraceProofObject>,
    pub supports: Vec<NodeRelationProjection>,
    pub missing_refs: Vec<String>,
    pub missing_bodies: Vec<String>,
    pub incomplete_entries: Vec<String>,
    pub clock_unknown_entries: Vec<String>,
    pub complete_groups: Vec<u32>,
    pub incomplete_groups: Vec<u32>,
    pub stop: Option<TraceSearchStop>,
    /// Canonical UTF-8 body bytes loaded once per unique object. Not wire bytes.
    pub body_bytes: u64,
}
