//! The session store this process actually uses.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::view::domain::{ViewId, ViewSession, ViewState};
use crate::view::ports::{SlotOutcome, ViewSessionStore};

/// A view nobody has touched for this long is forgotten. The state is a
/// camera position, not a record — it is ephemeral on purpose,
/// process-scoped and never written to the store. Persisting it would
/// quietly turn a camera position into memory.
const VIEW_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Open views this process keeps at once. Any id opens a view, and a browser
/// page on another port of this host could otherwise open thousands that
/// live six hours each. A browser holds one and an agent a few; when a new
/// view needs a slot, the one nobody touched for longest is forgotten.
const MAX_OPEN_VIEWS: usize = 64;

struct StoredSession {
    session: ViewSession,
    touched: SystemTime,
}

/// Every open view in this process, under one lock — which is what makes a
/// use case's whole judgment atomic against concurrent hands on the loom.
#[derive(Default)]
pub struct InMemorySessions {
    views: Mutex<HashMap<String, StoredSession>>,
}

impl InMemorySessions {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ViewSessionStore for InMemorySessions {
    fn operate<R>(
        &self,
        id: &ViewId,
        operation: impl FnOnce(Option<&mut ViewSession>) -> SlotOutcome<R>,
    ) -> R {
        let mut views = self.views.lock().expect("view registry poisoned");
        prune(&mut views);
        match views.get_mut(id.as_str()) {
            Some(stored) => {
                stored.touched = SystemTime::now();
                operation(Some(&mut stored.session)).result
            }
            None => {
                let outcome = operation(None);
                if let Some(session) = outcome.open {
                    evict_least_recent(&mut views, MAX_OPEN_VIEWS - 1);
                    views.insert(
                        id.as_str().to_string(),
                        StoredSession {
                            session,
                            touched: SystemTime::now(),
                        },
                    );
                }
                outcome.result
            }
        }
    }

    fn read(&self, id: &ViewId) -> Option<ViewState> {
        let mut views = self.views.lock().expect("view registry poisoned");
        // Reading counts as touching: a view someone is watching does not
        // expire under them, and one nobody reads eventually goes.
        prune(&mut views);
        let stored = views.get_mut(id.as_str())?;
        stored.touched = SystemTime::now();
        Some(stored.session.state().clone())
    }
}

/// Forgets the least recently touched views until at most `keep` remain.
fn evict_least_recent(views: &mut HashMap<String, StoredSession>, keep: usize) {
    while views.len() > keep {
        let Some(oldest) = views
            .iter()
            .min_by_key(|(_, stored)| stored.touched)
            .map(|(id, _)| id.clone())
        else {
            return;
        };
        views.remove(&oldest);
    }
}

fn prune(views: &mut HashMap<String, StoredSession>) {
    let now = SystemTime::now();
    views.retain(|_, stored| {
        now.duration_since(stored.touched)
            .map(|age| age < VIEW_TTL)
            .unwrap_or(true)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::ViewRegistry;

    #[test]
    fn opening_past_the_cap_forgets_the_view_touched_longest_ago() {
        let registry = ViewRegistry::new();
        registry.ensure_open(Some("watched"), Some("about:x".into()));
        for index in 0..MAX_OPEN_VIEWS * 3 {
            registry.ensure_open(Some(&format!("flood-{index}")), Some("about:x".into()));
            // A view someone reads stays fresh.
            assert!(registry.view_state(Some("watched")).is_some());
        }
        assert!(registry.view_state(Some("watched")).is_some());
        assert!(registry.view_state(Some("flood-0")).is_none());
        let last = format!("flood-{}", MAX_OPEN_VIEWS * 3 - 1);
        assert!(registry.view_state(Some(&last)).is_some());
    }
}
