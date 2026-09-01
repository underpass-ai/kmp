//! The thin composition root of the view context.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::view::adapters::{
    InMemorySessions, StaticOverlayCatalog, SystemWallClock, TokioChangeBell,
};
use crate::view::application::Applied;
use crate::view::application::commands::{ApplyIntentCommand, OpenViewCommand};
use crate::view::application::use_cases::{
    ApplyViewIntent, AwaitViewChange, GetViewState, OpenView, UndoViewMove,
};
use crate::view::domain::{OverlayName, ViewError, ViewState};
use crate::view::ports::OverlayCatalog;

/// One loom, two faces: the MCP tools and the HTTP viewer are two boundaries
/// of one process looking at one view registry. This root wires the
/// in-process adapters to the use cases and holds nothing else — threading a
/// handle through five generic layers would only disguise that there is one.
pub struct ViewRegistry {
    sessions: InMemorySessions,
    bell: TokioChangeBell,
    wall_clock: SystemWallClock,
    overlays: StaticOverlayCatalog,
}

impl Default for ViewRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewRegistry {
    /// A registry with no open views and an empty overlay catalog.
    pub fn new() -> Self {
        Self {
            sessions: InMemorySessions::new(),
            bell: TokioChangeBell::new(),
            wall_clock: SystemWallClock::new(),
            overlays: StaticOverlayCatalog::new(),
        }
    }

    /// The process-wide registry both faces share.
    pub fn shared() -> &'static Arc<ViewRegistry> {
        static SHARED: OnceLock<Arc<ViewRegistry>> = OnceLock::new();
        SHARED.get_or_init(|| Arc::new(ViewRegistry::new()))
    }

    /// Publishes the exact telemetry vocabulary mounted beside this
    /// registry. The MCP face uses it to distinguish a requested overlay
    /// from a name the actual viewer reader cannot resolve.
    pub fn set_available_overlays(&self, names: impl IntoIterator<Item = String>) {
        self.overlays
            .publish(names.into_iter().map(OverlayName::new).collect());
    }

    /// Whether the mounted telemetry reader resolves this series.
    pub fn overlay_available(&self, name: &str) -> bool {
        self.overlays.contains(&OverlayName::new(name))
    }

    /// Opens a view against the revision the caller actually saw.
    pub fn open_view(&self, command: OpenViewCommand) -> Result<ViewState, ViewError> {
        OpenView {
            store: &self.sessions,
            bell: &self.bell,
            wall_clock: &self.wall_clock,
        }
        .execute(command)
    }

    /// Opens a view, or rehydrates the one already under that id — the
    /// unconditional open the browser's report path uses to make sure a
    /// loom exists before reporting onto it.
    pub fn ensure_open(&self, view_id: Option<&str>, about: Option<String>) -> ViewState {
        self.open_view(OpenViewCommand {
            view_id: view_id.map(str::to_string),
            about,
            expected_revision: None,
            actor: "agent".to_string(),
            explanation: None,
        })
        .expect("an unconditional view open cannot conflict")
    }

    /// The semantic state under a view id, if one is open there.
    pub fn view_state(&self, view_id: Option<&str>) -> Option<ViewState> {
        GetViewState {
            store: &self.sessions,
        }
        .execute(view_id)
    }

    /// Applies one intent atomically, under optimistic concurrency and
    /// idempotency.
    pub fn apply_intent(&self, command: ApplyIntentCommand) -> Result<Applied, ViewError> {
        ApplyViewIntent {
            store: &self.sessions,
            bell: &self.bell,
            wall_clock: &self.wall_clock,
        }
        .execute(command)
    }

    /// Steps back one change.
    pub fn undo(&self, view_id: Option<&str>, actor: &str) -> Result<ViewState, ViewError> {
        UndoViewMove {
            store: &self.sessions,
            bell: &self.bell,
            wall_clock: &self.wall_clock,
        }
        .execute(view_id, actor)
    }

    /// Resolves as soon as the view's revision moves past `since`, or when
    /// `patience` runs out.
    pub async fn changed_since(
        &self,
        view_id: Option<&str>,
        since: u64,
        patience: Duration,
    ) -> Option<ViewState> {
        AwaitViewChange {
            store: &self.sessions,
            bell: &self.bell,
        }
        .execute(view_id, since, patience)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::application::dto::{ProjectionDto, ViewIntentDto};

    fn registry() -> ViewRegistry {
        let registry = ViewRegistry::new();
        registry.ensure_open(Some("t"), Some("about:x".into()));
        registry
    }

    fn intent_command(intent: ViewIntentDto) -> ApplyIntentCommand {
        ApplyIntentCommand {
            view_id: Some("t".to_string()),
            intent,
            actor: "agent:test".to_string(),
            ..ApplyIntentCommand::default()
        }
    }

    #[test]
    fn overlays_are_honored_view_state() {
        let registry = registry();
        let applied = registry
            .apply_intent(intent_command(ViewIntentDto {
                projection: Some(ProjectionDto {
                    overlays: Some(vec!["projection_lag".into()]),
                    ..ProjectionDto::default()
                }),
                ..ViewIntentDto::default()
            }))
            .expect("applies");
        assert!(applied.unhonored.is_empty());
        let overlays = applied
            .state
            .projection
            .overlays
            .expect("overlays are state");
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].as_str(), "projection_lag");
    }

    #[test]
    fn the_catalog_answers_for_exactly_what_was_published() {
        let registry = ViewRegistry::new();
        registry.set_available_overlays(vec!["noise_ratio".to_string()]);
        assert!(registry.overlay_available("noise_ratio"));
        assert!(!registry.overlay_available("projection_lag"));
    }

    #[test]
    fn an_intent_on_a_view_nobody_opened_is_refused() {
        let registry = ViewRegistry::new();
        let refused = registry.apply_intent(ApplyIntentCommand {
            view_id: Some("never-opened".to_string()),
            intent: ViewIntentDto {
                clock: Some("observed".into()),
                ..ViewIntentDto::default()
            },
            actor: "agent:test".to_string(),
            ..ApplyIntentCommand::default()
        });
        assert!(matches!(
            refused,
            Err(crate::view::domain::ViewError::UnknownView(_))
        ));
        assert!(registry.undo(Some("never-opened"), "human").is_err());
    }

    #[test]
    fn an_expectation_of_a_view_that_never_existed_is_refused() {
        let registry = ViewRegistry::new();
        let refused = registry.open_view(crate::view::application::commands::OpenViewCommand {
            view_id: Some("vacant".to_string()),
            about: Some("about:x".to_string()),
            expected_revision: Some(7),
            actor: "human".to_string(),
            explanation: None,
        });
        assert!(matches!(
            refused,
            Err(crate::view::domain::ViewError::UnknownView(_))
        ));
        // Expecting absence is the one expectation a vacant slot satisfies.
        let opened = registry
            .open_view(crate::view::application::commands::OpenViewCommand {
                view_id: Some("vacant".to_string()),
                about: Some("about:x".to_string()),
                expected_revision: Some(0),
                actor: "human".to_string(),
                explanation: None,
            })
            .expect("absence satisfied");
        assert_eq!(opened.view_revision.value(), 1);
    }

    #[test]
    fn a_registry_undo_steps_back_one_applied_intent() {
        let registry = registry();
        let moved = registry
            .apply_intent(intent_command(ViewIntentDto {
                clock: Some("observed".into()),
                ..ViewIntentDto::default()
            }))
            .expect("applies");
        let undone = registry.undo(Some("t"), "human").expect("undo");
        assert!(undone.view_revision > moved.state.view_revision);
        assert_eq!(undone.clock.as_str(), "occurred");
        assert!(!undone.can_undo);
    }

    /// The boundary may hand the digest in, or leave the registry to take
    /// it — either way a retry under the same key is the same intent.
    #[test]
    fn a_replay_is_honest_even_when_the_registry_computes_the_digest() {
        let registry = registry();
        let command = || ApplyIntentCommand {
            view_id: Some("t".to_string()),
            idempotency_key: Some("same".to_string()),
            intent: ViewIntentDto {
                clock: Some("ingested".into()),
                ..ViewIntentDto::default()
            },
            actor: "agent:test".to_string(),
            ..ApplyIntentCommand::default()
        };
        let first = registry.apply_intent(command()).expect("first");
        let replay = registry.apply_intent(command()).expect("replay");
        assert!(first.applied);
        assert!(!replay.applied, "a retry is not a second move");
        assert_eq!(replay.state.view_revision, first.state.view_revision);
    }

    /// The bell has to be listened for before the state is read, or a change
    /// landing in between is missed and the caller waits out its patience.
    #[tokio::test]
    async fn a_change_between_the_read_and_the_wait_is_not_missed() {
        let registry = std::sync::Arc::new(ViewRegistry::new());
        registry.ensure_open(Some("t"), Some("about:x".into()));
        let since = registry
            .view_state(Some("t"))
            .expect("state")
            .view_revision
            .value();
        let writer = std::sync::Arc::clone(&registry);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = writer.apply_intent(ApplyIntentCommand {
                view_id: Some("t".to_string()),
                intent: ViewIntentDto {
                    clock: Some("ingested".into()),
                    ..ViewIntentDto::default()
                },
                actor: "agent:test".to_string(),
                ..ApplyIntentCommand::default()
            });
        });
        let started = tokio::time::Instant::now();
        let state = registry
            .changed_since(Some("t"), since, std::time::Duration::from_secs(5))
            .await
            .expect("a state");
        assert!(state.view_revision.value() > since);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "the waiter woke on the bell, not on the timeout"
        );
    }
}
