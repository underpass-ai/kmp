//! Wire intent → domain patch, with the vocabulary refusals.

use crate::view::application::dto::{
    FocusDto, LabelSelectorDto, ProjectionDto, TimeRangeDto, ViewIntentDto,
};
use crate::view::domain::{
    AboutId, AboutLayers, Clock, DimensionName, Focus, FocusWindow, LabelOperator, LabelSelection,
    MemoryRef, OverlayName, ProjectionSettings, RelationClass, SearchQuery, SemanticZoom,
    Timestamp, TraceSelection, ViewError, ViewPatch,
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
    // Labels beside a whole-projection replacement are ignored by the
    // aggregate, so they are not validated either — the same rule as the
    // window beside a whole focus.
    let projection_labels = match (&intent.projection, intent.projection_labels.as_ref()) {
        (None, Some(labels)) => Some(labels.as_deref().map(label_selections).transpose()?),
        _ => None,
    };
    let focus = intent.focus.as_ref().map(focus_from).transpose()?;
    // A window beside a whole-focus replacement is ignored by the aggregate,
    // so it is not validated either — exactly as it always was.
    let focus_window = match (&intent.focus, intent.focus_window.as_ref()) {
        (None, Some(window)) => Some(window_from(window)?),
        _ => None,
    };
    let projection_abouts = if intent.projection.is_none() {
        intent
            .projection_abouts
            .as_ref()
            .map(|abouts| {
                abouts
                    .as_ref()
                    .map(|names| AboutLayers::new(names.clone()))
                    .transpose()
            })
            .transpose()?
    } else {
        None
    };
    let projection_zoom = if intent.projection.is_none() {
        intent
            .projection_zoom
            .as_ref()
            .map(|zoom| {
                zoom.as_ref()
                    .map(|name| {
                        SemanticZoom::parse(name).ok_or_else(|| {
                            ViewError::Invalid(format!("`{name}` is not a semantic zoom level"))
                        })
                    })
                    .transpose()
            })
            .transpose()?
    } else {
        None
    };
    Ok(ViewPatch {
        projection_zoom,
        projection_abouts,
        about: intent.about.clone().map(AboutId::new),
        clock,
        focus,
        focus_window,
        projection,
        projection_labels,
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
        abouts: projection
            .abouts
            .as_ref()
            .map(|names| AboutLayers::new(names.clone()))
            .transpose()?,
        semantic_zoom,
        dimensions: projection
            .dimensions
            .as_ref()
            .map(|dimensions| dimensions.iter().cloned().map(DimensionName::new).collect()),
        labels: projection
            .labels
            .as_deref()
            .map(label_selections)
            .transpose()?,
        relation_classes,
        overlays: projection
            .overlays
            .as_ref()
            .map(|overlays| overlays.iter().cloned().map(OverlayName::new).collect()),
    })
}

/// The kernel's four operators and nothing else; a selector whose shape
/// means nothing is refused by the value object itself.
fn label_selections(selectors: &[LabelSelectorDto]) -> Result<Vec<LabelSelection>, ViewError> {
    selectors
        .iter()
        .map(|selector| {
            let operator = LabelOperator::parse(&selector.op).ok_or_else(|| {
                ViewError::Invalid(format!(
                    "`{}` is not a label operator; KMP reads {}",
                    selector.op,
                    LabelOperator::NAMES.join(", ")
                ))
            })?;
            LabelSelection::new(&selector.key, operator, selector.values.iter().cloned())
        })
        .collect()
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
            projection_abouts: None,
            projection_zoom: None,
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
                abouts: None,
                semantic_zoom: Some("atlas".into()),
                dimensions: Some(vec!["timeline".into()]),
                labels: Some(vec![LabelSelectorDto {
                    key: "task".into(),
                    op: "in".into(),
                    values: vec!["launch".into()],
                }]),
                relation_classes: Some(vec!["causal".into(), "structural".into()]),
                overlays: Some(vec!["noise_ratio".into()]),
            }),
            projection_labels: None,
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
        let labels = projection.labels.expect("labels");
        assert_eq!(labels[0].key(), "task");
        assert_eq!(labels[0].operator(), LabelOperator::In);
        assert_eq!(labels[0].values(), &["launch"]);
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
    fn a_label_operator_outside_the_vocabulary_is_refused() {
        let refused = view_patch_from_intent(&ViewIntentDto {
            projection: Some(ProjectionDto {
                labels: Some(vec![LabelSelectorDto {
                    key: "task".into(),
                    op: "like".into(),
                    values: vec!["launch".into()],
                }]),
                ..ProjectionDto::default()
            }),
            ..ViewIntentDto::default()
        });
        assert!(matches!(refused, Err(ViewError::Invalid(_))));
        let shapeless = view_patch_from_intent(&ViewIntentDto {
            projection_labels: Some(Some(vec![LabelSelectorDto {
                key: "task".into(),
                op: "exists".into(),
                values: vec!["launch".into()],
            }])),
            ..ViewIntentDto::default()
        });
        assert!(matches!(shapeless, Err(ViewError::Invalid(_))));
    }

    /// A person's chips travel as `projection_labels`: they land as the
    /// labels alone, and beside a whole projection they are ignored rather
    /// than validated — the window's rule, applied to labels.
    #[test]
    fn a_persons_chips_replace_only_the_labels() {
        let patch = view_patch_from_intent(&ViewIntentDto {
            projection_labels: Some(Some(vec![LabelSelectorDto {
                key: "task".into(),
                op: "notexists".into(),
                values: Vec::new(),
            }])),
            ..ViewIntentDto::default()
        })
        .expect("chips map");
        assert!(patch.projection.is_none());
        let labels = patch.projection_labels.expect("labels facet").expect("set");
        assert_eq!(labels[0].operator(), LabelOperator::NotExists);
        let cleared = view_patch_from_intent(&ViewIntentDto {
            projection_labels: Some(None),
            ..ViewIntentDto::default()
        })
        .expect("clearing maps");
        assert_eq!(cleared.projection_labels, Some(None));
        let ignored = view_patch_from_intent(&ViewIntentDto {
            projection: Some(ProjectionDto::default()),
            projection_labels: Some(Some(vec![LabelSelectorDto {
                key: "task".into(),
                op: "like".into(),
                values: Vec::new(),
            }])),
            ..ViewIntentDto::default()
        })
        .expect("the ignored labels do not fail the intent");
        assert!(ignored.projection_labels.is_none());
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
