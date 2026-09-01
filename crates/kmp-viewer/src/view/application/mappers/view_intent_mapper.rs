//! Wire intent → domain patch, with the vocabulary refusals.

use crate::view::application::dto::{FocusDto, ProjectionDto, TimeRangeDto, ViewIntentDto};
use crate::view::domain::{
    AboutId, Clock, DimensionName, Focus, FocusWindow, MemoryRef, OverlayName, ProjectionSettings,
    RelationClass, SearchQuery, SemanticZoom, Timestamp, TraceSelection, ViewError, ViewPatch,
};

/// Turns one arrived intent into a domain patch, refusing vocabulary the
/// domain does not speak — in the same order the aggregate always refused
/// it: clock, zoom rung, relation classes, then the window's own invariant.
pub fn view_patch_from_intent(intent: &ViewIntentDto) -> Result<ViewPatch, ViewError> {
    let clock = match intent.clock.as_deref() {
        None => None,
        Some(name) => Some(Clock::parse(name).ok_or_else(|| {
            ViewError::Invalid(format!(
                "`{name}` is not a clock; KMP reads {}",
                Clock::NAMES.join(", ")
            ))
        })?),
    };
    let projection = intent
        .projection
        .as_ref()
        .map(projection_settings)
        .transpose()?;
    let focus = intent.focus.as_ref().map(focus_from).transpose()?;
    // A window beside a whole-focus replacement is ignored by the aggregate,
    // so it is not validated either — exactly as it always was.
    let focus_window = match (&intent.focus, intent.focus_window.as_ref()) {
        (None, Some(window)) => Some(window_from(window)?),
        _ => None,
    };
    Ok(ViewPatch {
        about: intent.about.clone().map(AboutId::new),
        clock,
        focus,
        focus_window,
        projection,
        selection: intent
            .selection
            .clone()
            .map(|selection| selection.map(MemoryRef::new)),
        trace: intent.trace.clone().map(|trace| {
            trace.map(|trace| TraceSelection {
                from: MemoryRef::new(trace.from),
                to: MemoryRef::new(trace.to),
            })
        }),
        search: intent
            .search
            .clone()
            .map(|search| search.map(SearchQuery::new)),
    })
}

fn projection_settings(projection: &ProjectionDto) -> Result<ProjectionSettings, ViewError> {
    let semantic_zoom = match projection.semantic_zoom.as_deref() {
        None => None,
        Some(name) => Some(SemanticZoom::parse(name).ok_or_else(|| {
            ViewError::Invalid(format!(
                "`{name}` is not a rung of the zoom ladder; it goes {}",
                SemanticZoom::NAMES.join(", ")
            ))
        })?),
    };
    let relation_classes = match projection.relation_classes.as_ref() {
        None => None,
        Some(classes) => Some(
            classes
                .iter()
                .map(|class| {
                    RelationClass::parse(class).ok_or_else(|| {
                        ViewError::Invalid(format!(
                            "`{class}` is not a relation class; KMP draws {}",
                            RelationClass::NAMES.join(", ")
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(ProjectionSettings {
        semantic_zoom,
        dimensions: projection
            .dimensions
            .as_ref()
            .map(|dimensions| dimensions.iter().cloned().map(DimensionName::new).collect()),
        relation_classes,
        overlays: projection
            .overlays
            .as_ref()
            .map(|overlays| overlays.iter().cloned().map(OverlayName::new).collect()),
    })
}

fn focus_from(focus: &FocusDto) -> Result<Focus, ViewError> {
    Ok(Focus {
        window: focus.time_range.as_ref().map(window_from).transpose()?,
        refs: focus.refs.iter().cloned().map(MemoryRef::new).collect(),
    })
}

fn window_from(range: &TimeRangeDto) -> Result<FocusWindow, ViewError> {
    FocusWindow::new(
        range.from.clone().map(Timestamp::new),
        range.to.clone().map(Timestamp::new),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::application::dto::TraceSelectionDto;

    #[test]
    fn a_full_intent_maps_every_facet_into_the_domain() {
        let patch = view_patch_from_intent(&ViewIntentDto {
            about: Some("about:x".into()),
            clock: Some("validity".into()),
            focus: Some(FocusDto {
                time_range: Some(TimeRangeDto {
                    from: Some("2026-08-31T16:49:00Z".into()),
                    to: Some("2026-08-31T17:39:00Z".into()),
                }),
                refs: vec!["decision:new".into()],
            }),
            focus_window: None,
            projection: Some(ProjectionDto {
                semantic_zoom: Some("atlas".into()),
                dimensions: Some(vec!["timeline".into()]),
                relation_classes: Some(vec!["causal".into(), "structural".into()]),
                overlays: Some(vec!["noise_ratio".into()]),
            }),
            selection: Some(Some("decision:new".into())),
            trace: Some(Some(TraceSelectionDto {
                from: "decision:new".into(),
                to: "success:old".into(),
            })),
            search: Some(None),
        })
        .expect("a valid intent maps");
        assert!(patch.touches_anything());
        assert_eq!(patch.clock.map(Clock::as_str), Some("validity"));
        let focus = patch.focus.expect("focus");
        assert_eq!(focus.refs.len(), 1);
        assert!(focus.window.is_some());
        let projection = patch.projection.expect("projection");
        assert_eq!(
            projection.semantic_zoom.map(SemanticZoom::as_str),
            Some("atlas")
        );
        assert_eq!(
            projection.relation_classes.map(|classes| classes.len()),
            Some(2)
        );
        assert_eq!(patch.selection, Some(Some(MemoryRef::new("decision:new"))));
        assert_eq!(patch.search, Some(None), "an explicit null clears");
        assert!(!ViewPatch::default().touches_anything());
    }

    #[test]
    fn a_backwards_window_in_a_focus_is_refused_at_the_boundary() {
        let refused = view_patch_from_intent(&ViewIntentDto {
            focus_window: Some(TimeRangeDto {
                from: Some("2026-08-28T00:00:00Z".into()),
                to: Some("2026-08-27T00:00:00Z".into()),
            }),
            ..ViewIntentDto::default()
        });
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
    }

    #[test]
    fn a_clock_the_kernel_does_not_keep_is_refused() {
        let refused = view_patch_from_intent(&ViewIntentDto {
            clock: Some("vibes".into()),
            ..ViewIntentDto::default()
        });
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
    }

    #[test]
    fn evidence_is_a_selection_state_not_a_zoom_rung() {
        let refused = view_patch_from_intent(&ViewIntentDto {
            projection: Some(ProjectionDto {
                semantic_zoom: Some("evidence".into()),
                ..ProjectionDto::default()
            }),
            ..ViewIntentDto::default()
        });
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
    }

    #[test]
    fn a_relation_class_outside_the_domain_vocabulary_is_refused() {
        let refused = view_patch_from_intent(&ViewIntentDto {
            projection: Some(ProjectionDto {
                relation_classes: Some(vec!["telepathic".into()]),
                ..ProjectionDto::default()
            }),
            ..ViewIntentDto::default()
        });
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
    }

    #[test]
    fn a_window_beside_a_whole_focus_is_ignored_not_validated() {
        let patch = view_patch_from_intent(&ViewIntentDto {
            focus: Some(FocusDto::default()),
            focus_window: Some(TimeRangeDto {
                from: Some("2026-08-28T00:00:00Z".into()),
                to: Some("2026-08-27T00:00:00Z".into()),
            }),
            ..ViewIntentDto::default()
        })
        .expect("the ignored window does not fail the intent");
        assert!(patch.focus_window.is_none());
    }
}
