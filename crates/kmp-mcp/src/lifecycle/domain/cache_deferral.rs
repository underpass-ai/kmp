use super::release_version::ReleaseVersion;

/// The cached releases a proved convergence superseded and left on disk.
///
/// Removing them the moment the new release is proved breaks a session that
/// is already open. A Codex session captures its skill catalog once, at the
/// start, and every path it captured points inside the version directory it
/// started from; deleting that directory leaves the session dispatching its
/// own skills at files that are no longer there, with no way back short of
/// restarting the host (#521). There is no liveness signal to consult
/// instead: the `.in_use` markers a removal survey reads are written by
/// Claude Code, not by KMP, and Codex writes none.
///
/// So a convergence names what it superseded and stops. This is not a report
/// that the disk shrank; it is the list the next process start collects, at
/// a moment when no session can still be reading it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CacheDeferral {
    deferred: Vec<ReleaseVersion>,
}

impl CacheDeferral {
    pub fn new(deferred: Vec<ReleaseVersion>) -> Self {
        Self { deferred }
    }

    /// Superseded, still on disk, and collected by the next process start.
    pub fn deferred(&self) -> &[ReleaseVersion] {
        &self.deferred
    }

    pub fn is_empty(&self) -> bool {
        self.deferred.is_empty()
    }
}
