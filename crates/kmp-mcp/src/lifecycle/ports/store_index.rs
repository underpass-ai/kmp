use std::path::PathBuf;

use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Outbound port for the machine-local note of where project stores live.
///
/// Project stores can be anywhere on disk, so they are remembered rather
/// than scanned for. The note is machine state about someone's filesystem:
/// it stays local and it never travels in a bundle. Which paths deserve to
/// stay in it is policy, and policy lives in the use cases — this port only
/// reads and rewrites the note.
pub trait StoreIndex: Send + Sync {
    /// Where the note lives, for the commands that account for it as a file.
    fn location(&self) -> PathBuf;

    /// The paths the note names, in the order they were first seen.
    /// `None` when no note has ever been written on this machine.
    fn remembered(&self) -> Option<Vec<PathBuf>>;

    /// Rewrite the note to name exactly these paths.
    fn replace(&self, paths: &[PathBuf]) -> Result<(), LifecycleError>;

    /// Remove the note entirely; an empty note is not worth keeping.
    fn erase(&self) -> Result<(), LifecycleError>;
}
