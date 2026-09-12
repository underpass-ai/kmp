use crate::{
    NodeRelationProjection, TraceBodyDelivery, TraceCompactSummary, TraceExpansionPlan,
    TraceExpansionRefusal, TraceProofObject, TraceSearchStop,
};

/// Completeness of the fetched declared proof, never semantic answer sufficiency.
/// Objects and support rows are shared tables, not repeated inside each route.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceProofResult {
    pub objects: Vec<TraceProofObject>,
    pub supports: Vec<NodeRelationProjection>,
    pub missing_refs: Vec<String>,
    /// Refs whose body the store does not hold. Absent evidence only: a body
    /// this response withheld is never listed here.
    pub missing_bodies: Vec<String>,
    pub incomplete_entries: Vec<String>,
    pub clock_unknown_entries: Vec<String>,
    pub complete_groups: Vec<u32>,
    pub incomplete_groups: Vec<u32>,
    pub stop: Option<TraceSearchStop>,
    /// Canonical UTF-8 body bytes actually loaded, once per unique object.
    /// Not wire bytes, and never a body this response chose not to read.
    pub body_bytes: u64,
    /// Identity of the selected proof table, computed before any body was
    /// read. `None` on the legacy unbounded read.
    pub manifest_id: Option<String>,
    /// How the bodies of this selection were delivered. `None` on the legacy
    /// unbounded read, where every present body is loaded.
    pub delivery: Option<TraceBodyDelivery>,
    /// Present only when the read asked for a compact presentation.
    pub compact: Option<TraceCompactSummary>,
    /// What to ask for next, decided over the whole selection before any
    /// pagination. `None` when nothing the store holds is still withheld.
    pub expansion_plan: Option<TraceExpansionPlan>,
    /// Set when a named expansion was refused. Objects, supports and every
    /// text are empty: nothing is joined across two different selections and
    /// nothing is delivered for a partly invalid batch.
    pub refusal: Option<TraceExpansionRefusal>,
}
