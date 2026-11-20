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

    /// Rewrite the note from what it names now, as one step no other
    /// process can interleave with. Stores opened at the same moment each
    /// read the note, added their own path and wrote it back, and every
    /// writer but the last was lost (N8).
    ///
    /// `edit` receives the paths named now (`None` when no note exists) and
    /// returns the paths to name, or `None` to leave the note as it is. An
    /// empty list removes the note; an empty note is not worth keeping. A
    /// note that exists but cannot be read is never rewritten.
    fn update(
        &self,
        edit: &mut dyn FnMut(Option<Vec<PathBuf>>) -> Option<Vec<PathBuf>>,
    ) -> Result<(), LifecycleError>;
}
