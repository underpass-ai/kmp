//! Domain state → wire snapshot.

use crate::view::application::dto::{
    FocusDto, ProjectionDto, ProvenanceDto, TimeRangeDto, TraceSelectionDto, ViewStateDto,
};
use crate::view::domain::{
    Focus, FocusWindow, ProjectionSettings, Provenance, Timestamp, TraceSelection, ViewState,
};

/// Renders one view state as the wire snapshot both faces receive. This is
/// the only place domain state becomes JSON-shaped, so the whole wire
/// contract — including the explicit nulls for cleared search and window
/// that [#463](https://github.com/underpass-ai/kmp/issues/463) demanded — is
/// reviewable here.
pub fn view_state_dto(state: &ViewState) -> ViewStateDto {
    ViewStateDto {
        view_id: state.view_id.as_str().to_string(),
        view_revision: state.view_revision.value(),
        about: state.about.as_ref().map(|about| about.as_str().to_string()),
        clock: state.clock.as_str().to_string(),
        focus: focus_dto(&state.focus),
        projection: projection_dto(&state.projection),
        selection: state
            .selection
            .as_ref()
            .map(|selection| selection.as_str().to_string()),
        trace: state.trace.as_ref().map(trace_dto),
        search: state
            .search
            .as_ref()
            .map(|search| search.as_str().to_string()),
        last_change: state.last_change.as_ref().map(provenance_dto),
        can_undo: state.can_undo,
    }
}

fn focus_dto(focus: &Focus) -> FocusDto {
    FocusDto {
        time_range: focus.window.as_ref().map(time_range_dto),
        refs: focus
            .refs
            .iter()
            .map(|reference| reference.as_str().to_string())
            .collect(),
    }
}

fn time_range_dto(window: &FocusWindow) -> TimeRangeDto {
    TimeRangeDto {
        from: window.from().map(Timestamp::as_str).map(str::to_string),
        to: window.to().map(Timestamp::as_str).map(str::to_string),
    }
}

fn projection_dto(projection: &ProjectionSettings) -> ProjectionDto {
    ProjectionDto {
        semantic_zoom: projection
            .semantic_zoom
            .map(|zoom| zoom.as_str().to_string()),
        dimensions: projection.dimensions.as_ref().map(|dimensions| {
            dimensions
                .iter()
                .map(|dimension| dimension.as_str().to_string())
                .collect()
        }),
        relation_classes: projection.relation_classes.as_ref().map(|classes| {
            classes
                .iter()
                .map(|class| class.as_str().to_string())
                .collect()
        }),
        overlays: projection.overlays.as_ref().map(|overlays| {
            overlays
                .iter()
                .map(|overlay| overlay.as_str().to_string())
                .collect()
        }),
    }
}

fn trace_dto(trace: &TraceSelection) -> TraceSelectionDto {
    TraceSelectionDto {
        from: trace.from.as_str().to_string(),
        to: trace.to.as_str().to_string(),
    }
}

fn provenance_dto(provenance: &Provenance) -> ProvenanceDto {
    ProvenanceDto {
        actor: provenance.actor.as_str().to_string(),
        explanation: provenance.explanation.clone(),
        idempotency_key: provenance
            .idempotency_key
            .as_ref()
            .map(|key| key.as_str().to_string()),
        at: provenance.at.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::domain::{
        AboutId, Actor, Clock, DimensionName, IdempotencyKey, MemoryRef, OverlayName,
        RelationClass, SearchQuery, SemanticZoom, ViewId,
    };

    /// The whole populated snapshot, byte for byte. This is the wire both
    /// faces read; a change to any mapped field name or value shape is a
    /// wire change and must fail here first.
    #[test]
    fn a_populated_state_maps_onto_exactly_the_wire_the_browser_reads() {
        let mut state = ViewState::opened(ViewId::from("t"), Some(AboutId::new("about:x")));
        state.clock = Clock::Observed;
        state.focus = Focus {
            window: Some(
                FocusWindow::new(
                    Some(Timestamp::new("2026-08-31T16:49:00Z")),
                    Some(Timestamp::new("2026-08-31T17:39:00Z")),
                )
                .expect("a forward window"),
            ),
            refs: vec![MemoryRef::new("decision:new")],
        };
        state.projection = ProjectionSettings {
            semantic_zoom: Some(SemanticZoom::Moment),
            dimensions: Some(vec![DimensionName::new("timeline")]),
            relation_classes: Some(vec![RelationClass::Causal, RelationClass::Evidential]),
            overlays: Some(vec![OverlayName::new("noise_ratio")]),
        };
        state.selection = Some(MemoryRef::new("decision:new"));
        state.trace = Some(TraceSelection {
            from: MemoryRef::new("decision:new"),
            to: MemoryRef::new("success:old"),
        });
        state.search = Some(SearchQuery::new("attempt-000005"));
        state.last_change = Some(Provenance {
            actor: Actor::named("agent:test"),
            explanation: Some("look here".to_string()),
            idempotency_key: Some(IdempotencyKey::new("k1")),
            at: Timestamp::new("2026-08-31T18:02:14Z"),
        });

        let wire = serde_json::to_value(view_state_dto(&state)).expect("state serializes");
        assert_eq!(
            wire,
            serde_json::json!({
                "view_id": "t",
                "view_revision": 1,
                "about": "about:x",
                "clock": "observed",
                "focus": {
                    "time_range": {
                        "from": "2026-08-31T16:49:00Z",
                        "to": "2026-08-31T17:39:00Z"
                    },
                    "refs": ["decision:new"]
                },
                "projection": {
                    "semantic_zoom": "moment",
                    "dimensions": ["timeline"],
                    "relation_classes": ["causal", "evidential"],
                    "overlays": ["noise_ratio"]
                },
                "selection": "decision:new",
                "trace": { "from": "decision:new", "to": "success:old" },
                "search": "attempt-000005",
                "last_change": {
                    "actor": "agent:test",
                    "explanation": "look here",
                    "idempotency_key": "k1",
                    "at": "2026-08-31T18:02:14Z"
                },
                "can_undo": false
            })
        );
    }

    /// #463's contract: a cleared search and a cleared window are explicit
    /// `null`s in the snapshot, never omissions — a full snapshot must tell
    /// the browser what to clear. Selection and trace stay omitted (the
    /// browser never held local state for them that an omission could
    /// strand), and empty refs stay omitted because absence already reads
    /// as emptiness there.
    #[test]
    fn cleared_search_and_window_are_explicit_nulls_in_the_snapshot() {
        let state = ViewState::opened(ViewId::from("t"), Some(AboutId::new("about:x")));
        let wire = serde_json::to_value(view_state_dto(&state)).expect("state serializes");
        let object = wire.as_object().expect("a snapshot is an object");
        assert!(
            object.contains_key("search") && wire["search"].is_null(),
            "a cleared search must be present as null (#463): {wire}"
        );
        let focus = wire["focus"].as_object().expect("focus is an object");
        assert!(
            focus.contains_key("time_range") && wire["focus"]["time_range"].is_null(),
            "a cleared window must be present as null (#463): {wire}"
        );
        assert!(!focus.contains_key("refs"), "empty refs stay omitted");
        assert!(!object.contains_key("selection"));
        assert!(!object.contains_key("trace"));
        assert_eq!(wire["view_id"], "t");
        assert_eq!(wire["view_revision"], 1);
        assert_eq!(wire["about"], "about:x");
        assert_eq!(wire["clock"], "occurred");
        assert_eq!(wire["can_undo"], false);
    }
}
