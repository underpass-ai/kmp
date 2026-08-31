//! The center of the view context. Every concept the loom reasons about is a
//! type here — a clock is a [`Clock`], a revision a [`ViewRevision`] — and
//! every invariant (optimistic concurrency, idempotent replay, reversible
//! moves, a window that ends after it begins) is enforced by the aggregate
//! itself, never by a caller remembering to check.
//!
//! Nothing in this module performs I/O, reads a wall clock, serializes, or
//! names a JSON key.

mod about_id;
mod actor;
mod clock;
mod dimension_name;
mod focus;
mod focus_window;
mod idempotency_claim;
mod idempotency_key;
mod idempotency_record;
mod intent_digest;
mod memory_ref;
mod overlay_name;
mod projection_settings;
mod provenance;
mod relation_class;
mod search_query;
mod semantic_zoom;
mod session_intent;
mod session_outcome;
mod timestamp;
mod trace_selection;
mod view_error;
mod view_id;
mod view_patch;
mod view_revision;
mod view_session;
mod view_state;

pub use about_id::AboutId;
pub use actor::Actor;
pub use clock::Clock;
pub use dimension_name::DimensionName;
pub use focus::Focus;
pub use focus_window::FocusWindow;
pub use idempotency_claim::IdempotencyClaim;
pub use idempotency_key::IdempotencyKey;
pub use idempotency_record::IdempotencyRecord;
pub use intent_digest::IntentDigest;
pub use memory_ref::MemoryRef;
pub use overlay_name::OverlayName;
pub use projection_settings::ProjectionSettings;
pub use provenance::Provenance;
pub use relation_class::RelationClass;
pub use search_query::SearchQuery;
pub use semantic_zoom::SemanticZoom;
pub use session_intent::SessionIntent;
pub use session_outcome::SessionOutcome;
pub use timestamp::Timestamp;
pub use trace_selection::TraceSelection;
pub use view_error::ViewError;
pub use view_id::{DEFAULT_VIEW_ID, ViewId};
pub use view_patch::ViewPatch;
pub use view_revision::ViewRevision;
pub use view_session::ViewSession;
pub use view_state::ViewState;
