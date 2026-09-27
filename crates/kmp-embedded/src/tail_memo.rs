use std::sync::{Arc, Mutex};

use crate::committed_tail::CommittedTail;

/// Where a commit-native bundle keeps its last publish. A cache, shared by
/// every clone of the bundle that guards the same paths: two bundles are
/// equal whatever each remembers.
#[derive(Debug, Clone, Default)]
pub(crate) struct TailMemo(Arc<Mutex<Option<CommittedTail>>>);

impl TailMemo {
    /// Takes what was remembered: a guarded write that fails or is
    /// abandoned leaves nothing, and the next one checks in full.
    pub(crate) fn take(&self) -> Option<CommittedTail> {
        self.0.lock().ok().and_then(|mut held| held.take())
    }

    pub(crate) fn set(&self, tail: Option<CommittedTail>) {
        if let Ok(mut held) = self.0.lock() {
            *held = tail;
        }
    }
}

impl PartialEq for TailMemo {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for TailMemo {}
