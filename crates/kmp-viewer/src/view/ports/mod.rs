//! What the application needs from the world, named from its own needs —
//! never from the shape of a library. Somewhere to keep sessions, a bell
//! that rings when one changes, a wall clock for attribution, and the
//! catalog of telemetry series the mounted process can actually draw.

mod change_bell;
mod overlay_catalog;
mod view_session_store;
mod wall_clock;

pub use change_bell::ChangeBell;
pub use overlay_catalog::OverlayCatalog;
pub use view_session_store::{SlotOutcome, ViewSessionStore};
pub use wall_clock::WallClock;
