//! When a move landed.

use crate::view::domain::Timestamp;

/// Reads the present instant for attribution. A port, so the domain's
/// provenance never reaches for the system clock itself and a test can hold
/// time still.
pub trait WallClock: Send + Sync {
    /// The present instant, in the kernel's notation.
    fn now(&self) -> Timestamp;
}
