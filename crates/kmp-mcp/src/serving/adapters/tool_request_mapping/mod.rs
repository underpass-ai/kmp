//! Shared MCP JSON-to-protobuf request mapping, independent of transport.

mod answer_policy;
mod json_fields;
mod memory_budget;
mod memory_enums;

mod ask;
mod condense;
mod dimensions;
mod ingest;
mod inspect;
mod near;
mod relabel;
mod relate;
mod temporal;
mod temporal_move;
mod trace;
mod trace_material;
mod trace_search;
mod trace_seek;
mod visual_projection;
mod wake;

pub(crate) use ask::AskRequestMapper;
pub(crate) use condense::CondenseRequestMapper;
pub(crate) use ingest::IngestRequestMapper;
pub(crate) use inspect::InspectRequestMapper;
pub(crate) use near::NearRequestMapper;
pub(crate) use relabel::RelabelRequestMapper;
pub(crate) use relate::RelateRequestMapper;
pub(crate) use temporal_move::TemporalMoveRequestMapper;
pub(crate) use trace::TraceRequestMapper;
pub(crate) use visual_projection::VisualProjectionRequestMapper;
pub(crate) use wake::WakeRequestMapper;
