//! The projection bounded context: kernel responses as the JSON a caller
//! reads, held to the byte ceilings the surface publishes. One mapper per
//! response family; the budgets trim after the mapper has spoken.

pub(crate) mod ingest_projection;
pub(crate) mod inspect_budget;
pub(crate) mod inspect_projection;
mod recall_budget_audit;
pub(crate) mod recall_projection;
pub(crate) mod relate_projection;
mod rendering;
mod serialized_size;
pub(crate) mod temporal_projection;
mod test_support;
pub(crate) mod trace_projection;
pub(crate) mod visual_projection;
mod wording;

pub(crate) use ingest_projection::{dry_run_ingest_from_plan, ingest_from_response};
pub(crate) use inspect_budget::enforce_inspect_output_budget;
pub(crate) use inspect_projection::inspect_from_response;
#[cfg(test)]
pub(crate) use recall_projection::enforce_recall_output_budget;
pub(crate) use recall_projection::{
    ask_from_response, try_enforce_recall_output_budget, wake_from_response,
};
pub(crate) use relate_projection::relate_from_response;
pub(crate) use temporal_projection::{enforce_temporal_output_budget, temporal_from_response};
pub(crate) use trace_projection::trace_from_response;
pub(crate) use visual_projection::visual_projection_from_response;
