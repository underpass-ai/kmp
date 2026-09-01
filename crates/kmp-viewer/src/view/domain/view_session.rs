//! The aggregate root: one open view and everything that makes it safe to
//! share.
//!
//! Three properties let a person and an agent hold the same loom:
//!
//! * **Its own revision.** `view_revision` is not `memory_revision`. Moving a
//!   window is not remembering anything.
//! * **Optimistic concurrency.** Every mutation may carry an expected
//!   revision; if the human moved the loom while the agent was thinking, the
//!   agent is told so and rebases rather than yanking the view from under
//!   them.
//! * **Idempotency.** A retried intent with the same key is the same intent,
//!   not a second one — and a key reused for different content is a
//!   collision, not a replay.
//!
//! The session is pure: it judges intents and remembers history. Where it is
//! stored, for how long, and who is told when it moves are the ports'
//! business.

use crate::view::domain::about_id::AboutId;
use crate::view::domain::actor::Actor;
use crate::view::domain::idempotency_claim::IdempotencyClaim;
use crate::view::domain::idempotency_record::IdempotencyRecord;
use crate::view::domain::provenance::Provenance;
use crate::view::domain::session_intent::SessionIntent;
use crate::view::domain::session_outcome::SessionOutcome;
use crate::view::domain::timestamp::Timestamp;
use crate::view::domain::view_error::ViewError;
use crate::view::domain::view_id::ViewId;
use crate::view::domain::view_revision::ViewRevision;
use crate::view::domain::view_state::ViewState;

/// How many moves stay reversible. Enough for a session of gestures; not a
/// transcript.
const HISTORY_LIMIT: usize = 32;
/// How many honored keys are remembered for replay detection.
const IDEMPOTENCY_LIMIT: usize = 64;

/// One open view: its state, its reversible history, and the idempotency
/// keys it has honored.
#[derive(Clone, Debug)]
pub struct ViewSession {
    state: ViewState,
    history: Vec<ViewState>,
    honored: Vec<IdempotencyRecord>,
}

impl ViewSession {
    /// Opens a fresh session on a view id, optionally already over a memory.
    pub fn opened(view_id: ViewId, about: Option<AboutId>) -> Self {
        Self {
            state: ViewState::opened(view_id, about),
            history: Vec::new(),
            honored: Vec::new(),
        }
    }

    /// The state both faces read.
    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// Re-opens the session, possibly onto another about. Same about is a
    /// rehydrate and changes nothing; another about is a destructive camera
    /// reset, so unlike a rehydrate it is concurrency-checked and
    /// attributed. Returns whether the view moved.
    pub fn reopen(
        &mut self,
        about: Option<AboutId>,
        expected: Option<ViewRevision>,
        actor: Actor,
        explanation: Option<String>,
        at: Timestamp,
    ) -> Result<bool, ViewError> {
        let Some(about) = about else {
            return Ok(false);
        };
        if self.state.about.as_ref() == Some(&about) {
            return Ok(false);
        }
        if let Some(expected) = expected
            && expected != self.state.view_revision
        {
            return Err(ViewError::Conflict {
                expected,
                actual: self.state.view_revision,
                current: Box::new(self.state.clone()),
            });
        }
        let revision = self.state.view_revision.next();
        let view_id = self.state.view_id.clone();
        self.state = ViewState::opened(view_id, Some(about));
        self.state.view_revision = revision;
        self.state.last_change = Some(Provenance {
            actor,
            explanation: explanation.or_else(|| Some("opened a different about".to_string())),
            idempotency_key: None,
            at,
        });
        self.history.clear();
        self.state.can_undo = false;
        Ok(true)
    }

    /// Judges one intent under optimistic concurrency and idempotency.
    /// Every field the patch leaves alone survives untouched.
    pub fn judge(&mut self, intent: SessionIntent) -> Result<SessionOutcome, ViewError> {
        // A replayed key is the same intent, answered the same way — before
        // the revision check, because the intent under that key already
        // landed and a now-stale expectation must not turn success into a
        // conflict. A key reused for another patch is a collision.
        if let Some(claim) = intent.idempotency.as_ref()
            && let Some(record) = self
                .honored
                .iter()
                .find(|record| record.claim.key == claim.key)
        {
            if claim.digest != record.claim.digest {
                return Err(ViewError::IdempotencyConflict {
                    key: claim.key.clone(),
                });
            }
            let mut state = self.state.clone();
            state.view_revision = state.view_revision.max(record.revision);
            return Ok(SessionOutcome {
                state,
                applied: false,
            });
        }

        if let Some(expected) = intent.expected
            && expected != self.state.view_revision
        {
            return Err(ViewError::Conflict {
                expected,
                actual: self.state.view_revision,
                current: Box::new(self.state.clone()),
            });
        }

        if !intent.patch.touches_anything() {
            let revision = self.state.view_revision;
            self.remember(intent.idempotency, revision);
            return Ok(SessionOutcome {
                state: self.state.clone(),
                applied: false,
            });
        }

        let mut next = self.state.clone();
        next.apply(intent.patch);
        if next == self.state {
            let revision = self.state.view_revision;
            self.remember(intent.idempotency, revision);
            return Ok(SessionOutcome {
                state: self.state.clone(),
                applied: false,
            });
        }

        self.history.push(self.state.clone());
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }

        next.view_revision = next.view_revision.next();
        next.can_undo = true;
        next.last_change = Some(Provenance {
            actor: intent.actor,
            explanation: intent.explanation,
            idempotency_key: intent.idempotency.as_ref().map(|claim| claim.key.clone()),
            at: intent.at,
        });
        self.state = next;
        let revision = self.state.view_revision;
        self.remember(intent.idempotency, revision);
        Ok(SessionOutcome {
            state: self.state.clone(),
            applied: true,
        })
    }

    /// Steps back one change. Every visual action is reversible, including
    /// the agent's — that is what makes handing it the wheel safe.
    pub fn undo(&mut self, actor: Actor, at: Timestamp) -> Result<ViewState, ViewError> {
        let Some(previous) = self.history.pop() else {
            return Err(ViewError::Invalid("nothing to undo on this view".into()));
        };
        let revision = self.state.view_revision.next();
        self.state = previous;
        self.state.view_revision = revision;
        self.state.can_undo = !self.history.is_empty();
        self.state.last_change = Some(Provenance {
            actor,
            explanation: Some("undo".into()),
            idempotency_key: None,
            at,
        });
        Ok(self.state.clone())
    }

    fn remember(&mut self, claim: Option<IdempotencyClaim>, revision: ViewRevision) {
        let Some(claim) = claim else {
            return;
        };
        self.honored.push(IdempotencyRecord { claim, revision });
        if self.honored.len() > IDEMPOTENCY_LIMIT {
            self.honored.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::domain::clock::Clock;
    use crate::view::domain::focus::Focus;
    use crate::view::domain::focus_window::FocusWindow;
    use crate::view::domain::idempotency_key::IdempotencyKey;
    use crate::view::domain::intent_digest::IntentDigest;
    use crate::view::domain::memory_ref::MemoryRef;
    use crate::view::domain::projection_settings::ProjectionSettings;
    use crate::view::domain::view_patch::ViewPatch;
    use crate::view::domain::{OverlayName, SearchQuery};

    fn session() -> ViewSession {
        ViewSession::opened(ViewId::from("t"), Some(AboutId::new("about:x")))
    }

    fn at() -> Timestamp {
        Timestamp::new("2026-08-31T00:00:00Z")
    }

    fn claim(key: &str, digest: &str) -> Option<IdempotencyClaim> {
        Some(IdempotencyClaim {
            key: IdempotencyKey::new(key),
            digest: IntentDigest::new(digest),
        })
    }

    fn intent(patch: ViewPatch) -> SessionIntent {
        SessionIntent {
            expected: None,
            idempotency: None,
            patch,
            actor: Actor::named("agent:test"),
            explanation: None,
            at: at(),
        }
    }

    #[test]
    fn an_intent_moves_the_view_and_says_who_moved_it() {
        let mut session = session();
        let outcome = session
            .judge(SessionIntent {
                explanation: Some("look here".into()),
                ..intent(ViewPatch {
                    clock: Some(Clock::Observed),
                    ..ViewPatch::default()
                })
            })
            .expect("applies");
        assert!(outcome.applied);
        assert_eq!(outcome.state.clock, Clock::Observed);
        let provenance = outcome.state.last_change.expect("provenance");
        assert_eq!(provenance.actor.as_str(), "agent:test");
        assert_eq!(provenance.explanation.as_deref(), Some("look here"));
    }

    #[test]
    fn a_replayed_key_is_the_same_intent_not_a_second_one() {
        let mut session = session();
        let original_revision = session.state().view_revision;
        let patch = || ViewPatch {
            clock: Some(Clock::Ingested),
            ..ViewPatch::default()
        };
        let first = session
            .judge(SessionIntent {
                idempotency: claim("same", "digest-1"),
                ..intent(patch())
            })
            .expect("first");
        let later = session
            .judge(SessionIntent {
                expected: Some(first.state.view_revision),
                idempotency: claim("later", "digest-2"),
                actor: Actor::human(),
                ..intent(ViewPatch {
                    selection: Some(Some(MemoryRef::new("entry:1"))),
                    ..ViewPatch::default()
                })
            })
            .expect("the view moves after the original intent");
        let second = session
            .judge(SessionIntent {
                expected: Some(original_revision),
                idempotency: claim("same", "digest-1"),
                ..intent(patch())
            })
            .expect("a true replay bypasses a now-stale expected revision");
        assert!(first.applied);
        assert!(!second.applied, "a retry is not a second move");
        assert_eq!(second.state.view_revision, later.state.view_revision);
        assert_eq!(second.state.clock, Clock::Ingested);
        assert_eq!(
            second.state.selection.as_ref().map(MemoryRef::as_str),
            Some("entry:1")
        );
    }

    #[test]
    fn a_reused_key_with_a_different_intent_is_a_conflict() {
        let mut session = session();
        let first = session
            .judge(SessionIntent {
                idempotency: claim("same", "digest-1"),
                ..intent(ViewPatch {
                    selection: Some(Some(MemoryRef::new("entry:1"))),
                    ..ViewPatch::default()
                })
            })
            .expect("first intent applies");

        let collision = session.judge(SessionIntent {
            idempotency: claim("same", "digest-2"),
            ..intent(ViewPatch {
                search: Some(Some(SearchQuery::new("another intent"))),
                ..ViewPatch::default()
            })
        });
        assert!(matches!(
            collision,
            Err(ViewError::IdempotencyConflict { ref key }) if key.as_str() == "same"
        ));

        let current = session.state();
        assert_eq!(current.view_revision, first.state.view_revision);
        assert_eq!(
            current.selection.as_ref().map(MemoryRef::as_str),
            Some("entry:1")
        );
        assert_eq!(current.search, None, "the colliding intent did not land");
    }

    #[test]
    fn a_fresh_key_that_reasserts_the_present_is_not_a_move() {
        let mut session = session();
        let patch = || ViewPatch {
            selection: Some(Some(MemoryRef::new("entry:1"))),
            ..ViewPatch::default()
        };
        let first = session
            .judge(SessionIntent {
                idempotency: claim("first", "digest-1"),
                explanation: Some("select".into()),
                ..intent(patch())
            })
            .expect("first selection");
        let second = session
            .judge(SessionIntent {
                expected: Some(first.state.view_revision),
                idempotency: claim("fresh-key", "digest-1"),
                actor: Actor::named("agent:other"),
                explanation: Some("reassert".into()),
                ..intent(patch())
            })
            .expect("no-op reassertion");

        assert!(!second.applied);
        assert_eq!(second.state.view_revision, first.state.view_revision);
        assert_eq!(second.state.last_change, first.state.last_change);
        let undone = session
            .undo(Actor::human(), at())
            .expect("one real move to undo");
        assert_eq!(undone.selection, None);
        assert!(!undone.can_undo, "the no-op did not add duplicate history");
    }

    #[test]
    fn a_stale_about_switch_cannot_erase_a_prepared_view() {
        let mut session = session();
        let prepared = session
            .judge(SessionIntent {
                idempotency: claim("pulse", "digest-1"),
                explanation: Some("align the quality pulse".into()),
                ..intent(ViewPatch {
                    projection: Some(ProjectionSettings {
                        overlays: Some(vec![OverlayName::new("noise_ratio")]),
                        ..ProjectionSettings::default()
                    }),
                    ..ViewPatch::default()
                })
            })
            .expect("agent prepares the view")
            .state;

        let stale = session.reopen(
            Some(AboutId::new("about:other")),
            Some(ViewRevision::from(prepared.view_revision.value() - 1)),
            Actor::human(),
            None,
            at(),
        );
        assert!(matches!(stale, Err(ViewError::Conflict { .. })));
        let still_prepared = session.state();
        assert_eq!(
            still_prepared.about.as_ref().map(AboutId::as_str),
            Some("about:x")
        );
        assert_eq!(
            still_prepared.projection.overlays,
            Some(vec![OverlayName::new("noise_ratio")])
        );

        let switched = session
            .reopen(
                Some(AboutId::new("about:other")),
                Some(prepared.view_revision),
                Actor::human(),
                None,
                at(),
            )
            .expect("a deliberate current switch applies");
        assert!(switched, "the switch moved the view");
        let state = session.state();
        assert_eq!(
            state.about.as_ref().map(AboutId::as_str),
            Some("about:other")
        );
        assert!(
            state
                .last_change
                .as_ref()
                .expect("reset provenance")
                .actor
                .is_human()
        );
    }

    /// The human moved while the agent was thinking. The agent is told, and
    /// told what the view looks like now, instead of yanking it back.
    #[test]
    fn a_stale_expected_revision_conflicts_and_hands_back_the_present() {
        let mut session = session();
        let stale = session.state().view_revision;
        session
            .judge(SessionIntent {
                idempotency: claim("human", "digest-1"),
                actor: Actor::human(),
                ..intent(ViewPatch {
                    selection: Some(Some(MemoryRef::new("entry:1"))),
                    ..ViewPatch::default()
                })
            })
            .expect("human moves");
        let conflict = session.judge(SessionIntent {
            expected: Some(stale),
            idempotency: claim("agent", "digest-2"),
            ..intent(ViewPatch {
                clock: Some(Clock::Validity),
                ..ViewPatch::default()
            })
        });
        match conflict {
            Err(ViewError::Conflict {
                expected,
                actual,
                current,
            }) => {
                assert_eq!(expected, stale);
                assert!(actual > stale);
                assert_eq!(
                    current.selection.as_ref().map(MemoryRef::as_str),
                    Some("entry:1")
                );
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn every_move_is_reversible() {
        let mut session = session();
        session
            .judge(intent(ViewPatch {
                clock: Some(Clock::Observed),
                ..ViewPatch::default()
            }))
            .expect("applies");
        let undone = session.undo(Actor::human(), at()).expect("undo");
        assert_eq!(undone.clock, Clock::Occurred);
        assert!(!undone.can_undo, "one move, one undo");
    }

    /// A person panning moves the window. It does not retract the refs an
    /// agent asked to frame — those are its intent, and it must still be
    /// able to read back what it asked for.
    #[test]
    fn a_human_pan_moves_the_window_without_dropping_the_focused_refs() {
        let mut session = session();
        session
            .judge(SessionIntent {
                explanation: Some("frame these".into()),
                ..intent(ViewPatch {
                    focus: Some(Focus {
                        window: None,
                        refs: vec![
                            MemoryRef::new("decision:one"),
                            MemoryRef::new("evidence:two"),
                        ],
                    }),
                    ..ViewPatch::default()
                })
            })
            .expect("agent frames refs");
        let after = session
            .judge(SessionIntent {
                actor: Actor::human(),
                ..intent(ViewPatch {
                    focus_window: Some(
                        FocusWindow::new(
                            Some(Timestamp::new("2026-08-26T00:00:00Z")),
                            Some(Timestamp::new("2026-08-27T00:00:00Z")),
                        )
                        .expect("a forward window"),
                    ),
                    ..ViewPatch::default()
                })
            })
            .expect("human pans");
        assert_eq!(after.state.focus.refs.len(), 2, "the intent survives a pan");
        assert_eq!(
            after
                .state
                .focus
                .window
                .expect("window")
                .to()
                .map(Timestamp::as_str),
            Some("2026-08-27T00:00:00Z")
        );
    }
}
