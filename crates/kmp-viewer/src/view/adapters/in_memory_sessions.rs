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

fn prune(views: &mut HashMap<String, StoredSession>) {
    let now = SystemTime::now();
    views.retain(|_, stored| {
        now.duration_since(stored.touched)
            .map(|age| age < VIEW_TTL)
            .unwrap_or(true)
    });
}
