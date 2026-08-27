//! The view aggregate: what the human and the agent are both looking at.
//!
//! ChronoLoom's agent control is declarative. An agent never says "move the
//! camera to x=438"; it says "focus these refs, on occurred time, with a five
//! minute window, causal and evidential relations". That intention lands
//! here, in a small aggregate with its own revision, and the browser follows
//! it — so the same state is legible to a person, to an agent, and to a test.
//!
//! Three properties make it safe to share:
//!
//! * **Its own revision.** `view_revision` is not `memory_revision`. Moving a
//!   window is not remembering anything.
//! * **Optimistic concurrency.** Every mutation carries `expected_revision`;
//!   if the human moved the loom while the agent was thinking, the agent is
//!   told so and rebases rather than yanking the view from under them.
//! * **Idempotency.** A retried intent with the same key is the same intent,
//!   not a second one.
//!
//! It is ephemeral on purpose: process-scoped, TTL'd, never written to the
//! store. Persisting it would quietly turn a camera position into memory.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// The view a host opens when it does not name one — one window, one loom.
pub const DEFAULT_VIEW_ID: &str = "default";

const HISTORY_LIMIT: usize = 32;
const IDEMPOTENCY_LIMIT: usize = 64;
/// A view nobody has touched for this long is forgotten. The state is a
/// camera position, not a record.
const VIEW_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Which clock the loom's axis reads. KMP has no single clock, so the view
/// has to say which one it means.
pub const CLOCKS: [&str; 4] = ["occurred", "observed", "ingested", "validity"];
/// The rungs of the semantic-zoom ladder an intent may ask for.
pub const ZOOMS: [&str; 4] = ["atlas", "episode", "moment", "evidence"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_zoom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_classes: Option<Vec<String>>,
    /// Accepted and echoed so an intent is not silently mangled, but nothing
    /// renders them yet: the Observability Pulse needs a telemetry query port
    /// KMP does not publish. Reported as unhonored rather than pretended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlays: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSelection {
    pub from: String,
    pub to: String,
}

/// Who moved the view and why — shown to the human, so an agent can never
/// rearrange what they are looking at anonymously.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewState {
    pub view_id: String,
    pub view_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub clock: String,
    pub focus: Focus,
    pub projection: Projection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<Provenance>,
    pub can_undo: bool,
}

impl ViewState {
    fn new(view_id: &str, about: Option<String>) -> Self {
        Self {
            view_id: view_id.to_string(),
            view_revision: 1,
            about,
            clock: "occurred".to_string(),
            focus: Focus::default(),
            projection: Projection::default(),
            selection: None,
            trace: None,
            search: None,
            last_change: None,
            can_undo: false,
        }
    }
}

/// One intent's worth of change. Every field is optional: an intent says what
/// it means to change and stays silent about the rest, so two agents editing
/// different facets do not clobber each other's.
#[derive(Clone, Debug, Default)]
pub struct ViewPatch {
    pub about: Option<String>,
    pub clock: Option<String>,
    pub focus: Option<Focus>,
    pub projection: Option<Projection>,
    /// `Some(None)` clears the selection; `None` leaves it alone.
    pub selection: Option<Option<String>>,
    pub trace: Option<Option<TraceSelection>>,
    pub search: Option<Option<String>>,
}

impl ViewPatch {
    fn touches_anything(&self) -> bool {
        self.about.is_some()
            || self.clock.is_some()
            || self.focus.is_some()
            || self.projection.is_some()
            || self.selection.is_some()
            || self.trace.is_some()
            || self.search.is_some()
    }
}

/// Why an intent did not apply. A conflict is not a failure of the agent; it
/// is the human having moved. It carries the current state so the caller can
/// rebase without a second round trip.
#[derive(Clone, Debug)]
pub enum ViewError {
    UnknownView(String),
    Conflict {
        expected: u64,
        actual: u64,
        current: Box<ViewState>,
    },
    Invalid(String),
}

/// What an application produced: the new state, and whether this call was the
/// one that produced it (a replayed idempotency key is honest about it).
#[derive(Clone, Debug)]
pub struct Applied {
    pub state: ViewState,
    pub applied: bool,
    pub unhonored: Vec<String>,
}

struct ViewEntry {
    state: ViewState,
    history: Vec<ViewState>,
    applied_keys: Vec<(String, u64)>,
    touched: SystemTime,
}

/// Every open view in this process, and a bell that rings when one changes.
pub struct ViewRegistry {
    views: Mutex<HashMap<String, ViewEntry>>,
    bell: Notify,
}

impl Default for ViewRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewRegistry {
    pub fn new() -> Self {
        Self {
            views: Mutex::new(HashMap::new()),
            bell: Notify::new(),
        }
    }

    /// The process-wide registry. The MCP tools and the HTTP viewer are two
    /// faces of one process looking at one loom; threading a handle through
    /// five generic layers would only disguise that.
    pub fn shared() -> &'static Arc<ViewRegistry> {
        static SHARED: OnceLock<Arc<ViewRegistry>> = OnceLock::new();
        SHARED.get_or_init(|| Arc::new(ViewRegistry::new()))
    }

    /// Opens a view, or rehydrates the one already under that id.
    pub fn open(&self, view_id: Option<&str>, about: Option<String>) -> ViewState {
        let id = view_id.unwrap_or(DEFAULT_VIEW_ID).to_string();
        let mut views = self.views.lock().expect("view registry poisoned");
        prune(&mut views);
        let entry = views.entry(id.clone()).or_insert_with(|| ViewEntry {
            state: ViewState::new(&id, about.clone()),
            history: Vec::new(),
            applied_keys: Vec::new(),
            touched: SystemTime::now(),
        });
        entry.touched = SystemTime::now();
        // Re-opening on another about is a fresh loom, not a merge.
        if let Some(about) = about
            && entry.state.about.as_deref() != Some(about.as_str())
        {
            let revision = entry.state.view_revision + 1;
            entry.state = ViewState::new(&id, Some(about));
            entry.state.view_revision = revision;
            entry.history.clear();
            entry.state.can_undo = false;
        }
        let state = entry.state.clone();
        drop(views);
        self.bell.notify_waiters();
        state
    }

    pub fn get(&self, view_id: &str) -> Option<ViewState> {
        let views = self.views.lock().expect("view registry poisoned");
        views.get(view_id).map(|entry| entry.state.clone())
    }

    /// Applies one intent atomically, under optimistic concurrency and
    /// idempotency. Every field the patch leaves alone survives untouched.
    pub fn apply(
        &self,
        view_id: &str,
        expected_revision: Option<u64>,
        idempotency_key: Option<&str>,
        patch: ViewPatch,
        actor: &str,
        explanation: Option<&str>,
    ) -> Result<Applied, ViewError> {
        if let Some(clock) = patch.clock.as_deref()
            && !CLOCKS.contains(&clock)
        {
            return Err(ViewError::Invalid(format!(
                "`{clock}` is not a clock; KMP reads {}",
                CLOCKS.join(", ")
            )));
        }
        if let Some(zoom) = patch
            .projection
            .as_ref()
            .and_then(|projection| projection.semantic_zoom.as_deref())
            && !ZOOMS.contains(&zoom)
        {
            return Err(ViewError::Invalid(format!(
                "`{zoom}` is not a rung of the zoom ladder; it goes {}",
                ZOOMS.join(", ")
            )));
        }

        let mut views = self.views.lock().expect("view registry poisoned");
        prune(&mut views);
        let Some(entry) = views.get_mut(view_id) else {
            return Err(ViewError::UnknownView(view_id.to_string()));
        };
        entry.touched = SystemTime::now();

        // A replayed key is the same intent, answered the same way.
        if let Some(key) = idempotency_key
            && let Some((_, revision)) = entry.applied_keys.iter().find(|(seen, _)| seen == key)
        {
            let mut state = entry.state.clone();
            state.view_revision = state.view_revision.max(*revision);
            return Ok(Applied {
                state,
                applied: false,
                unhonored: Vec::new(),
            });
        }

        if let Some(expected) = expected_revision
            && expected != entry.state.view_revision
        {
            return Err(ViewError::Conflict {
                expected,
                actual: entry.state.view_revision,
                current: Box::new(entry.state.clone()),
            });
        }

        if !patch.touches_anything() {
            return Ok(Applied {
                state: entry.state.clone(),
                applied: false,
                unhonored: Vec::new(),
            });
        }

        entry.history.push(entry.state.clone());
        if entry.history.len() > HISTORY_LIMIT {
            entry.history.remove(0);
        }

        let mut unhonored = Vec::new();
        let state = &mut entry.state;
        if let Some(about) = patch.about {
            state.about = Some(about);
        }
        if let Some(clock) = patch.clock {
            state.clock = clock;
        }
        if let Some(focus) = patch.focus {
            state.focus = focus;
        }
        if let Some(projection) = patch.projection {
            if let Some(overlays) = projection.overlays.as_ref()
                && !overlays.is_empty()
            {
                unhonored.push(format!(
                    "overlays {} are recorded but not drawn yet: the Observability Pulse needs \
                     a telemetry query port KMP does not publish",
                    overlays.join(", ")
                ));
            }
            state.projection = projection;
        }
        if let Some(selection) = patch.selection {
            state.selection = selection;
        }
        if let Some(trace) = patch.trace {
            state.trace = trace;
        }
        if let Some(search) = patch.search {
            state.search = search;
        }
        state.view_revision += 1;
        state.can_undo = true;
        state.last_change = Some(Provenance {
            actor: actor.to_string(),
            explanation: explanation.map(str::to_string),
            idempotency_key: idempotency_key.map(str::to_string),
            at: now_iso(),
        });

        if let Some(key) = idempotency_key {
            entry
                .applied_keys
                .push((key.to_string(), entry.state.view_revision));
            if entry.applied_keys.len() > IDEMPOTENCY_LIMIT {
                entry.applied_keys.remove(0);
            }
        }

        let state = entry.state.clone();
        drop(views);
        self.bell.notify_waiters();
        Ok(Applied {
            state,
            applied: true,
            unhonored,
        })
    }

    /// Steps back one change. Every visual action is reversible, including
    /// the agent's — that is what makes handing it the wheel safe.
    pub fn undo(&self, view_id: &str, actor: &str) -> Result<ViewState, ViewError> {
        let mut views = self.views.lock().expect("view registry poisoned");
        let Some(entry) = views.get_mut(view_id) else {
            return Err(ViewError::UnknownView(view_id.to_string()));
        };
        entry.touched = SystemTime::now();
        let Some(previous) = entry.history.pop() else {
            return Err(ViewError::Invalid("nothing to undo on this view".into()));
        };
        let revision = entry.state.view_revision + 1;
        entry.state = previous;
        entry.state.view_revision = revision;
        entry.state.can_undo = !entry.history.is_empty();
        entry.state.last_change = Some(Provenance {
            actor: actor.to_string(),
            explanation: Some("undo".into()),
            idempotency_key: None,
            at: now_iso(),
        });
        let state = entry.state.clone();
        drop(views);
        self.bell.notify_waiters();
        Ok(state)
    }

    /// Resolves as soon as the view's revision moves past `since`, or when
    /// `patience` runs out — the browser's long poll, so an agent's intent
    /// reaches the screen without anyone polling in a loop.
    pub async fn changed_since(
        &self,
        view_id: &str,
        since: u64,
        patience: Duration,
    ) -> Option<ViewState> {
        let deadline = tokio::time::Instant::now() + patience;
        loop {
            let state = self.get(view_id)?;
            if state.view_revision > since {
                return Some(state);
            }
            let waiting = self.bell.notified();
            if tokio::time::timeout_at(deadline, waiting).await.is_err() {
                return self.get(view_id);
            }
        }
    }
}

fn prune(views: &mut HashMap<String, ViewEntry>) {
    let now = SystemTime::now();
    views.retain(|_, entry| {
        now.duration_since(entry.touched)
            .map(|age| age < VIEW_TTL)
            .unwrap_or(true)
    });
}

fn now_iso() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    crate::views::rfc3339_utc(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ViewRegistry {
        let registry = ViewRegistry::new();
        registry.open(Some("t"), Some("about:x".into()));
        registry
    }

    #[test]
    fn an_intent_moves_the_view_and_says_who_moved_it() {
        let registry = registry();
        let patch = ViewPatch {
            clock: Some("observed".into()),
            ..ViewPatch::default()
        };
        let applied = registry
            .apply(
                "t",
                None,
                Some("k1"),
                patch,
                "agent:test",
                Some("look here"),
            )
            .expect("applies");
        assert!(applied.applied);
        assert_eq!(applied.state.clock, "observed");
        let provenance = applied.state.last_change.expect("provenance");
        assert_eq!(provenance.actor, "agent:test");
        assert_eq!(provenance.explanation.as_deref(), Some("look here"));
    }

    #[test]
    fn a_replayed_key_is_the_same_intent_not_a_second_one() {
        let registry = registry();
        let patch = || ViewPatch {
            clock: Some("ingested".into()),
            ..ViewPatch::default()
        };
        let first = registry
            .apply("t", None, Some("same"), patch(), "agent:test", None)
            .expect("first");
        let second = registry
            .apply("t", None, Some("same"), patch(), "agent:test", None)
            .expect("replay");
        assert!(first.applied);
        assert!(!second.applied, "a retry is not a second move");
        assert_eq!(first.state.view_revision, second.state.view_revision);
    }

    /// The human moved while the agent was thinking. The agent is told, and
    /// told what the view looks like now, instead of yanking it back.
    #[test]
    fn a_stale_expected_revision_conflicts_and_hands_back_the_present() {
        let registry = registry();
        let stale = registry.get("t").expect("state").view_revision;
        registry
            .apply(
                "t",
                None,
                Some("human"),
                ViewPatch {
                    selection: Some(Some("entry:1".into())),
                    ..ViewPatch::default()
                },
                "human",
                None,
            )
            .expect("human moves");
        let conflict = registry.apply(
            "t",
            Some(stale),
            Some("agent"),
            ViewPatch {
                clock: Some("validity".into()),
                ..ViewPatch::default()
            },
            "agent:test",
            None,
        );
        match conflict {
            Err(ViewError::Conflict {
                expected,
                actual,
                current,
            }) => {
                assert_eq!(expected, stale);
                assert!(actual > stale);
                assert_eq!(current.selection.as_deref(), Some("entry:1"));
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn every_move_is_reversible() {
        let registry = registry();
        registry
            .apply(
                "t",
                None,
                None,
                ViewPatch {
                    clock: Some("observed".into()),
                    ..ViewPatch::default()
                },
                "agent:test",
                None,
            )
            .expect("applies");
        let undone = registry.undo("t", "human").expect("undo");
        assert_eq!(undone.clock, "occurred");
        assert!(!undone.can_undo, "one move, one undo");
    }

    #[test]
    fn a_clock_the_kernel_does_not_keep_is_refused() {
        let registry = registry();
        let refused = registry.apply(
            "t",
            None,
            None,
            ViewPatch {
                clock: Some("vibes".into()),
                ..ViewPatch::default()
            },
            "agent:test",
            None,
        );
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
    }

    #[test]
    fn overlays_are_recorded_but_reported_as_undrawn() {
        let registry = registry();
        let applied = registry
            .apply(
                "t",
                None,
                None,
                ViewPatch {
                    projection: Some(Projection {
                        overlays: Some(vec!["projection_lag".into()]),
                        ..Projection::default()
                    }),
                    ..ViewPatch::default()
                },
                "agent:test",
                None,
            )
            .expect("applies");
        assert_eq!(applied.unhonored.len(), 1, "silence here would be a lie");
    }
}
