//! The wire truth. `serde` lives here and nowhere deeper: what these structs
//! serialize to is exactly what the browser and the MCP tools see, byte for
//! byte, and a change to a derive attribute here is a wire change.

mod focus_dto;
mod projection_dto;
mod provenance_dto;
mod time_range_dto;
mod trace_selection_dto;
mod view_intent_dto;
mod view_state_dto;

pub use focus_dto::FocusDto;
pub use projection_dto::ProjectionDto;
pub use provenance_dto::ProvenanceDto;
pub use time_range_dto::TimeRangeDto;
pub use trace_selection_dto::TraceSelectionDto;
pub use view_intent_dto::ViewIntentDto;
pub use view_state_dto::ViewStateDto;
