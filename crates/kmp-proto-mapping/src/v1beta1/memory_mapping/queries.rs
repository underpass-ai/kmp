use kmp_application::{
    AskMemoryQuery, InspectMemoryQuery, MAX_RELATE_PAGE_ENTRIES, MAX_TRACE_PAGE_ENTRIES,
    RelateMemoryQuery, RelatePageRequest, TemporalIncludeOptions, TemporalMemoryQuery,
    TraceMemoryQuery, TracePageRequest, WakeMemoryQuery,
};
use kmp_domain::{
    TemporalAxis, TemporalCursor, TemporalDirection, TemporalInterval, TemporalSelection,
};
use kmp_proto::v1beta1::TemporalCursor as ProtoTemporalCursor;
use kmp_proto::v1beta1::TemporalInterval as ProtoTemporalInterval;
use kmp_proto::v1beta1::{
    AskRequest, InspectInclude, InspectRequest, PageRequest, RelateRequest, TemporalInclude,
    TemporalLimit, TemporalMoveRequest, TemporalNearRequest, TraceRequest, WakeRequest,
};

use super::dimensions::domain_dimension_selection;
use super::scalars::{
    ProtoMappingResult, answer_policy_from_proto, invalid_argument, max_tier_from_detail,
    memory_detail_level, non_empty, proto_timestamp_to_sort_string, temporal_axis_from_proto,
};

pub fn wake_query_from_proto(request: WakeRequest) -> ProtoMappingResult<WakeMemoryQuery> {
    let budget = request.budget.unwrap_or_default();
    validate_recall_budget(&budget)?;
    Ok(WakeMemoryQuery {
        about: request.about.clone(),
        role: non_empty(request.role).unwrap_or_else(|| "agent".to_string()),
        intent: non_empty(request.intent)
            .unwrap_or_else(|| format!("continue from live kernel memory `{}`", request.about)),
        dimensions: domain_dimension_selection(request.dimensions)?,
        token_budget: if budget.tokens == 0 {
            1600
        } else {
            budget.tokens
        },
        depth: if budget.depth == 0 { 2 } else { budget.depth },
        max_tier: max_tier_from_detail(memory_detail_level(budget.detail)?),
        max_entries: (budget.max_entries != 0).then_some(budget.max_entries as usize),
        temporal: temporal_selection_from_proto(request.as_of, request.interval, request.axis)?,
    })
}

pub fn ask_query_from_proto(request: AskRequest) -> ProtoMappingResult<AskMemoryQuery> {
    let budget = request.budget.unwrap_or_default();
    validate_recall_budget(&budget)?;
    let temporal = temporal_selection_from_proto(request.as_of, request.interval, request.axis)?;
    Ok(AskMemoryQuery {
        about: request.about,
        question: request.question,
        asked_as: non_empty(request.asked_as),
        answer_policy: answer_policy_from_proto(request.answer_policy)?,
        dimensions: domain_dimension_selection(request.dimensions)?,
        token_budget: if budget.tokens == 0 {
            2400
        } else {
            budget.tokens
        },
        depth: if budget.depth == 0 { 2 } else { budget.depth },
        max_tier: max_tier_from_detail(memory_detail_level(budget.detail)?),
        max_entries: (budget.max_entries != 0).then_some(budget.max_entries as usize),
        temporal,
    })
}

pub fn relate_query_from_proto(request: RelateRequest) -> ProtoMappingResult<RelateMemoryQuery> {
    let budget = request.budget.unwrap_or_default();
    validate_recall_budget(&budget)?;
    Ok(RelateMemoryQuery {
        about: request.about,
        dimensions: domain_dimension_selection(request.dimensions)?,
        temporal: temporal_selection_from_proto(None, request.interval, request.axis)?,
        token_budget: if budget.tokens == 0 {
            2400
        } else {
            budget.tokens
        },
        depth: if budget.depth == 0 { 2 } else { budget.depth },
        max_tier: max_tier_from_detail(memory_detail_level(budget.detail)?),
        page: relate_page_from_proto(request.page)?,
    })
}

fn relate_page_from_proto(value: Option<PageRequest>) -> ProtoMappingResult<RelatePageRequest> {
    let Some(page) = value else {
        return Ok(RelatePageRequest::default());
    };
    let entries = if page.entries == 0 {
        None
    } else {
        let entries = page.entries as usize;
        if entries > MAX_RELATE_PAGE_ENTRIES {
            return Err(invalid_argument(format!(
                "relate page.entries must be <= {MAX_RELATE_PAGE_ENTRIES}"
            )));
        }
        Some(entries)
    };
    let cursor = match non_empty(page.cursor) {
        Some(cursor) => Some(cursor.parse::<usize>().map_err(|_| {
            invalid_argument("relate page.cursor must be a next_cursor returned by Relate")
        })?),
        None => None,
    };
    Ok(RelatePageRequest { entries, cursor })
}

/// The instants a recall stands on, from the three request fields that name
/// them. An instant and a span are exclusive, and a clock alone has nothing
/// to select on: naming one without the other would be a request that
/// changes nothing, and a request that changes nothing is a mistake the
/// caller would rather hear about.
fn temporal_selection_from_proto(
    as_of: Option<ProtoTemporalCursor>,
    interval: Option<ProtoTemporalInterval>,
    axis: i32,
) -> ProtoMappingResult<TemporalSelection> {
    let axis = temporal_axis_from_proto(axis)?;
    match (as_of, interval) {
        (Some(_), Some(_)) => Err(invalid_argument(
            "as_of and interval are exclusive: stand at an instant or within a span, not both",
        )),
        (Some(cursor), None) => {
            let cursor = domain_cursor_from_proto(&cursor)?;
            TemporalSelection::as_of(cursor, axis)
                .map_err(|error| invalid_argument(error.to_string()))
        }
        (None, Some(interval)) => {
            let interval = TemporalInterval::new(
                proto_timestamp_to_sort_string(interval.start),
                proto_timestamp_to_sort_string(interval.end),
            )
            .map_err(|error| invalid_argument(error.to_string()))?;
            Ok(TemporalSelection::within(interval, axis))
        }
        (None, None) if axis != TemporalAxis::Default => Err(invalid_argument(
            "axis has nothing to select on without as_of or interval",
        )),
        (None, None) => Ok(TemporalSelection::Frontier),
    }
}

pub fn temporal_query_from_move_proto(
    request: TemporalMoveRequest,
    direction: TemporalDirection,
) -> ProtoMappingResult<TemporalMemoryQuery> {
    temporal_query(TemporalQueryParts {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
        direction,
    })
}

pub fn temporal_query_from_near_proto(
    request: TemporalNearRequest,
) -> ProtoMappingResult<TemporalMemoryQuery> {
    temporal_query(TemporalQueryParts {
        about: request.about,
        cursor: request.around,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
        axis: request.axis,
        direction: TemporalDirection::Near,
    })
}

pub fn trace_query_from_proto(request: TraceRequest) -> ProtoMappingResult<TraceMemoryQuery> {
    let budget = request.budget.unwrap_or_default();
    Ok(TraceMemoryQuery {
        about: request.about,
        from: request.from,
        to: request.to,
        role: non_empty(request.goal).unwrap_or_else(|| "tracer".to_string()),
        token_budget: if budget.tokens == 0 {
            1600
        } else {
            budget.tokens
        },
        page: trace_page_from_proto(request.page)?,
    })
}

pub fn inspect_query_from_proto(request: InspectRequest) -> ProtoMappingResult<InspectMemoryQuery> {
    let include = request.include.unwrap_or(InspectInclude {
        incoming: true,
        outgoing: true,
        details: true,
        raw: false,
    });
    Ok(InspectMemoryQuery {
        about: request.about,
        ref_id: request.r#ref,
        include_details: include.details,
        include_incoming: include.incoming,
        include_outgoing: include.outgoing,
        include_raw: include.raw,
    })
}

struct TemporalQueryParts {
    about: String,
    cursor: Option<kmp_proto::v1beta1::TemporalCursor>,
    dimensions: Option<kmp_proto::v1beta1::DimensionSelection>,
    window: Option<kmp_proto::v1beta1::TemporalWindow>,
    limit: Option<TemporalLimit>,
    include: Option<TemporalInclude>,
    budget: Option<kmp_proto::v1beta1::MemoryBudget>,
    axis: i32,
    direction: TemporalDirection,
}

fn temporal_query(parts: TemporalQueryParts) -> ProtoMappingResult<TemporalMemoryQuery> {
    let cursor = parts
        .cursor
        .ok_or_else(|| invalid_argument("temporal cursor is required"))?;
    let budget = parts.budget.unwrap_or_default();
    let limit_entries = parts
        .limit
        .as_ref()
        .and_then(|limit| (limit.entries != 0).then_some(limit.entries as usize));
    let limit_tokens = parts
        .limit
        .as_ref()
        .and_then(|limit| (limit.tokens != 0).then_some(limit.tokens));
    let detail = memory_detail_level(budget.detail)?;
    Ok(TemporalMemoryQuery {
        about: parts.about,
        direction: parts.direction,
        axis: temporal_axis_from_proto(parts.axis)?,
        cursor: domain_cursor_from_proto(&cursor)?,
        dimensions: domain_dimension_selection(parts.dimensions)?,
        window: parts
            .window
            .map(|window| {
                kmp_domain::TemporalWindow::new(
                    window.before_entries as usize,
                    window.after_entries as usize,
                )
            })
            .unwrap_or_default(),
        limit_entries,
        include: parts
            .include
            .map(temporal_include_from_proto)
            .transpose()?
            .unwrap_or_default(),
        token_budget: if let Some(tokens) = limit_tokens {
            tokens
        } else if budget.tokens == 0 {
            2400
        } else {
            budget.tokens
        },
        depth: if budget.depth == 0 { 3 } else { budget.depth },
        max_tier: max_tier_from_detail(detail),
    })
}

fn temporal_include_from_proto(
    value: TemporalInclude,
) -> ProtoMappingResult<TemporalIncludeOptions> {
    Ok(TemporalIncludeOptions {
        evidence: value.evidence,
        relations: value.relations,
        raw_refs: value.raw_refs,
    })
}

fn validate_recall_budget(budget: &kmp_proto::v1beta1::MemoryBudget) -> ProtoMappingResult<()> {
    if budget.max_bytes != 0 && budget.max_bytes < 512 {
        return Err(invalid_argument(
            "recall budget.max_bytes must be zero or at least 512",
        ));
    }
    Ok(())
}

fn trace_page_from_proto(value: Option<PageRequest>) -> ProtoMappingResult<TracePageRequest> {
    let Some(page) = value else {
        return Ok(TracePageRequest::default());
    };
    let entries = if page.entries == 0 {
        None
    } else {
        let entries = page.entries as usize;
        if entries > MAX_TRACE_PAGE_ENTRIES {
            return Err(invalid_argument(format!(
                "trace page.entries must be <= {MAX_TRACE_PAGE_ENTRIES}"
            )));
        }
        Some(entries)
    };
    let cursor = match non_empty(page.cursor) {
        Some(cursor) => Some(cursor.parse::<usize>().map_err(|_| {
            invalid_argument("trace page.cursor must be a next_cursor returned by Trace")
        })?),
        None => None,
    };
    Ok(TracePageRequest { entries, cursor })
}

fn domain_cursor_from_proto(
    value: &kmp_proto::v1beta1::TemporalCursor,
) -> ProtoMappingResult<TemporalCursor> {
    let has_ref = !value.r#ref.trim().is_empty();
    let has_time = value.time.is_some();
    let has_sequence = value.sequence.is_some();
    if [has_ref, has_time, has_sequence]
        .into_iter()
        .filter(|present| *present)
        .count()
        != 1
    {
        return Err(invalid_argument(
            "temporal cursor requires exactly one of ref, time, or sequence",
        ));
    }

    if has_ref {
        return TemporalCursor::ref_id(value.r#ref.clone())
            .map_err(|error| invalid_argument(error.to_string()));
    }
    if let Some(time) = value.time {
        return TemporalCursor::time(
            proto_timestamp_to_sort_string(Some(time)).unwrap_or_default(),
        )
        .map_err(|error| invalid_argument(error.to_string()));
    }
    TemporalCursor::sequence(value.sequence.unwrap_or_default())
        .map_err(|error| invalid_argument(error.to_string()))
}

#[cfg(test)]
mod tests {
    use kmp_domain::TemporalAxis;
    use kmp_proto::v1beta1::{TemporalAxis as ProtoTemporalAxis, TemporalCursor};

    use super::*;

    #[test]
    fn temporal_query_carries_the_selected_clock_into_the_domain() {
        let query = temporal_query_from_move_proto(
            TemporalMoveRequest {
                about: "project:kmp".to_string(),
                cursor: Some(TemporalCursor {
                    r#ref: "project:kmp:entry:one".to_string(),
                    ..Default::default()
                }),
                axis: ProtoTemporalAxis::Ingested as i32,
                ..Default::default()
            },
            TemporalDirection::Goto,
        )
        .expect("explicit axis should map");

        assert_eq!(query.axis, TemporalAxis::Ingested);
    }

    #[test]
    fn inspect_returns_direct_links_when_include_is_absent() {
        let query = inspect_query_from_proto(InspectRequest {
            about: "project:kmp".to_string(),
            r#ref: "decision:one".to_string(),
            include: None,
        })
        .expect("default inspect request should map");

        assert!(query.include_incoming);
        assert!(query.include_outgoing);
        assert!(query.include_details);
        assert!(!query.include_raw);
    }
}

#[cfg(test)]
mod temporal_selection_tests {
    use kmp_domain::{TemporalAxis, TemporalSelection};
    use kmp_proto::v1beta1::{AskRequest, TemporalAxis as ProtoAxis};
    use prost_types::Timestamp;

    use super::*;

    fn ask(
        as_of: Option<ProtoTemporalCursor>,
        interval: Option<ProtoTemporalInterval>,
        axis: i32,
    ) -> AskRequest {
        AskRequest {
            about: "about:x".to_string(),
            question: "what stood".to_string(),
            as_of,
            interval,
            axis,
            ..AskRequest::default()
        }
    }

    fn at(seconds: i64) -> Option<Timestamp> {
        Some(Timestamp { seconds, nanos: 0 })
    }

    #[test]
    fn an_instant_and_a_span_are_exclusive_and_a_clock_alone_selects_nothing() {
        let both = ask(
            Some(ProtoTemporalCursor {
                r#ref: String::new(),
                time: at(1_000),
                sequence: None,
            }),
            Some(ProtoTemporalInterval {
                start: at(0),
                end: at(2_000),
            }),
            0,
        );
        let error = ask_query_from_proto(both).expect_err("exclusive");
        assert!(error.message().contains("exclusive"), "{}", error.message());

        let clock_alone = ask(None, None, ProtoAxis::Observed as i32);
        let error = ask_query_from_proto(clock_alone).expect_err("nothing to select on");
        assert!(
            error.message().contains("nothing to select on"),
            "{}",
            error.message()
        );

        let neither = ask(None, None, 0);
        assert!(
            ask_query_from_proto(neither)
                .expect("frontier")
                .temporal
                .is_frontier()
        );
    }

    #[test]
    fn a_span_maps_to_the_domain_interval_and_a_sequence_cursor_is_refused() {
        let span = ask(
            None,
            Some(ProtoTemporalInterval {
                start: at(0),
                end: at(2_000),
            }),
            ProtoAxis::Validity as i32,
        );
        let query = ask_query_from_proto(span).expect("a span");
        assert_eq!(query.temporal.axis(), Some(TemporalAxis::Validity));
        assert!(query.temporal.interval().is_some());

        let empty = ask(
            None,
            Some(ProtoTemporalInterval {
                start: at(2_000),
                end: at(2_000),
            }),
            0,
        );
        assert!(
            ask_query_from_proto(empty).is_err(),
            "an empty span selects nothing"
        );

        let sequence = ask(
            Some(ProtoTemporalCursor {
                r#ref: String::new(),
                time: None,
                sequence: Some(3),
            }),
            None,
            0,
        );
        let error = ask_query_from_proto(sequence).expect_err("no instant");
        assert!(
            error.message().contains("names no instant"),
            "{}",
            error.message()
        );

        let by_ref = ask(
            Some(ProtoTemporalCursor {
                r#ref: "about:x:e1".to_string(),
                time: None,
                sequence: None,
            }),
            None,
            0,
        );
        assert!(matches!(
            ask_query_from_proto(by_ref).expect("as of a ref").temporal,
            TemporalSelection::AsOf { .. }
        ));
    }
}
