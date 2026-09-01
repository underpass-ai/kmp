//! The edge of the view context: the in-process implementations of its
//! ports, and the HTTP face the browser talks to.

mod http_view_routes;
mod in_memory_sessions;
mod static_overlay_catalog;
mod system_wall_clock;
mod tokio_change_bell;
mod view_error_status;

pub(crate) use http_view_routes::{view_get, view_open, view_report, view_undo};
pub use in_memory_sessions::InMemorySessions;
pub use static_overlay_catalog::StaticOverlayCatalog;
pub use system_wall_clock::SystemWallClock;
pub use tokio_change_bell::TokioChangeBell;
