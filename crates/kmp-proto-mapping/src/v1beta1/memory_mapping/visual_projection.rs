use std::collections::HashMap;

use kmp_application::{
    TemporalAxisView, TemporalCoordinateView, VisualLevelOfDetail, VisualProjectionQuery,
    VisualProjectionResult, VisualRelation,
};
use kmp_proto::v1beta1::{
    DimensionCoverage, MemoryRelation, MemorySemanticClass, PageInfo, ProjectVisualRequest,
    ProjectVisualResponse, TemporalAxis as ProtoTemporalAxis, TemporalCoordinate, TemporalCoverage,
    TemporalEntry, VisualBin, VisualCluster, VisualLevelOfDetail as ProtoVisualLevelOfDetail,
    VisualMetric,
};

use super::dimensions::domain_dimension_selection;
use super::scalars::{
    ProtoMappingResult, invalid_argument, non_empty, proto_confidence,
    proto_timestamp_to_sort_string, temporal_axis_from_proto, timestamp_from_sort_or_rfc3339,
};

const DEFAULT_VISUAL_BINS: usize = 64;
const DEFAULT_VISUAL_PAGE: usize = 512;

pub fn visual_projection_query_from_proto(
    request: ProjectVisualRequest,
) -> ProtoMappingResult<VisualProjectionQuery> {
    let from = proto_timestamp_to_sort_string(request.from)
        .ok_or_else(|| invalid_argument("visual projection from timestamp is required"))?;
    let to = proto_timestamp_to_sort_string(request.to)
        .ok_or_else(|| invalid_argument("visual projection to timestamp is required"))?;
    let level_of_detail = match ProtoVisualLevelOfDetail::try_from(request.level_of_detail)
        .map_err(|_| invalid_argument("visual projection level_of_detail is invalid"))?
    {
        ProtoVisualLevelOfDetail::Unspecified | ProtoVisualLevelOfDetail::Atlas => {
            VisualLevelOfDetail::Atlas
        }
        ProtoVisualLevelOfDetail::Episode => VisualLevelOfDetail::Episode,
        ProtoVisualLevelOfDetail::Moment => VisualLevelOfDetail::Moment,
    };
    let budget = request.budget.unwrap_or_default();
    let page = request.page.unwrap_or_default();

    Ok(VisualProjectionQuery {
        about: request.about,
        from,
        to,
        axis: temporal_axis_from_proto(request.axis)?,
        dimensions: domain_dimension_selection(request.dimensions)?,
        level_of_detail,
        bin_count: if request.bin_count == 0 {
            DEFAULT_VISUAL_BINS
        } else {
            request.bin_count as usize
        },
        page_entries: if page.entries == 0 {
            DEFAULT_VISUAL_PAGE
        } else {
            page.entries as usize
        },
        cursor: non_empty(page.cursor),
        depth: if budget.depth == 0 { 3 } else { budget.depth },
    })
}

pub fn visual_projection_response_from_result(
    result: VisualProjectionResult,
) -> ProjectVisualResponse {
    let included = result.included_dimensions.clone();
    let missing_dimensions = result.missing_dimensions.clone();
    let total = result.page.total;
    let returned_by_dimension = result.bins.iter().fold(
        std::collections::BTreeMap::<String, usize>::new(),
        |mut counts, bin| {
            *counts.entry(bin.dimension.clone()).or_default() += bin.total;
            counts
        },
    );
    ProjectVisualResponse {
        contract: result.contract,
        about: result.about,
        axis: proto_axis(result.axis) as i32,
        level_of_detail: proto_lod(result.level_of_detail) as i32,
        from: timestamp_from_sort_or_rfc3339(Some(&result.range.from)),
        to: timestamp_from_sort_or_rfc3339(Some(&result.range.to)),
        bins: result
            .bins
            .into_iter()
            .map(|bin| VisualBin {
                dimension: bin.dimension,
                from: timestamp_from_sort_or_rfc3339(Some(&bin.from)),
                to: timestamp_from_sort_or_rfc3339(Some(&bin.to)),
                entries: u32_saturating(bin.total),
                by_kind: u32_map(bin.by_kind),
            })
            .collect(),
        clusters: result
            .clusters
            .into_iter()
            .enumerate()
            .map(|(index, cluster)| VisualCluster {
                id: format!("visual-cluster-{index}"),
                dimension: cluster.dimension,
                from: timestamp_from_sort_or_rfc3339(Some(&cluster.from)),
                to: timestamp_from_sort_or_rfc3339(Some(&cluster.to)),
                entries: u32_saturating(cluster.total),
                refs: cluster.refs,
                by_kind: u32_map(cluster.by_kind),
            })
            .collect(),
        entries: result
            .entries
            .into_iter()
            .map(|entry| TemporalEntry {
                r#ref: entry.ref_id,
                kind: entry.kind,
                text: entry.text,
                coordinates: entry
                    .coordinates
                    .into_iter()
                    .map(proto_coordinate)
                    .collect(),
                metadata: HashMap::new(),
            })
            .collect(),
        relations: result.relations.into_iter().map(proto_relation).collect(),
        metrics: result
            .metrics
            .into_iter()
            .map(|metric| VisualMetric {
                name: metric.name,
                value: metric.value,
                unit: metric.unit,
                scope: metric.scope,
            })
            .collect(),
        coverage: Some(TemporalCoverage {
            requested: None,
            included: included.clone(),
            missing: missing_dimensions,
            dimensions: included
                .into_iter()
                .map(|dimension| DimensionCoverage {
                    returned: u32_saturating(
                        returned_by_dimension
                            .get(&dimension)
                            .copied()
                            .unwrap_or_default(),
                    ),
                    dimension,
                    present: true,
                })
                .collect(),
        }),
        revision: result.revision,
        content_hash: result.content_hash,
        page: Some(PageInfo {
            returned: u32_saturating(result.page.returned),
            total: u32_saturating(total),
            has_more: result.page.has_more,
            next_cursor: result.page.next_cursor.unwrap_or_default(),
        }),
        truncated: result.truncated,
        missing: result.missing,
    }
}

fn proto_coordinate(value: TemporalCoordinateView) -> TemporalCoordinate {
    TemporalCoordinate {
        dimension: value.dimension,
        scope_id: value.scope_id,
        occurred_at: timestamp_from_sort_or_rfc3339(value.occurred_at.as_deref()),
        observed_at: timestamp_from_sort_or_rfc3339(value.observed_at.as_deref()),
        ingested_at: timestamp_from_sort_or_rfc3339(value.ingested_at.as_deref()),
        valid_from: timestamp_from_sort_or_rfc3339(value.valid_from.as_deref()),
        valid_until: timestamp_from_sort_or_rfc3339(value.valid_until.as_deref()),
        sequence: value.sequence,
        rank: value.rank,
        metadata: HashMap::new(),
    }
}

fn proto_relation(value: VisualRelation) -> MemoryRelation {
    let semantic_class = match value.class.as_str() {
        "causal" => MemorySemanticClass::Causal,
        "motivational" => MemorySemanticClass::Motivational,
        "procedural" => MemorySemanticClass::Procedural,
        "evidential" => MemorySemanticClass::Evidential,
        "constraint" => MemorySemanticClass::Constraint,
        _ => MemorySemanticClass::Structural,
    };
    MemoryRelation {
        source_ref: value.from,
        target_ref: value.to,
        rel: value.rel,
        semantic_class: semantic_class as i32,
        why: value.why.unwrap_or_default(),
        evidence: value.evidence.unwrap_or_default(),
        confidence: proto_confidence(value.confidence.as_deref()) as i32,
        sequence: None,
        explanation: None,
        evidence_refs: Vec::new(),
    }
}

fn proto_axis(value: TemporalAxisView) -> ProtoTemporalAxis {
    match value {
        TemporalAxisView::Default => ProtoTemporalAxis::Unspecified,
        TemporalAxisView::Occurred => ProtoTemporalAxis::Occurred,
        TemporalAxisView::Observed => ProtoTemporalAxis::Observed,
        TemporalAxisView::Ingested => ProtoTemporalAxis::Ingested,
        TemporalAxisView::Validity => ProtoTemporalAxis::Validity,
    }
}

fn proto_lod(value: VisualLevelOfDetail) -> ProtoVisualLevelOfDetail {
    match value {
        VisualLevelOfDetail::Atlas => ProtoVisualLevelOfDetail::Atlas,
        VisualLevelOfDetail::Episode => ProtoVisualLevelOfDetail::Episode,
        VisualLevelOfDetail::Moment => ProtoVisualLevelOfDetail::Moment,
    }
}

fn u32_map(values: std::collections::BTreeMap<String, usize>) -> HashMap<String, u32> {
    values
        .into_iter()
        .map(|(key, value)| (key, u32_saturating(value)))
        .collect()
}

fn u32_saturating(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use kmp_proto::v1beta1::{ProjectVisualRequest, VisualLevelOfDetail};
    use prost_types::Timestamp;

    use super::*;

    #[test]
    fn visual_projection_query_is_range_and_lod_aware() {
        let query = visual_projection_query_from_proto(ProjectVisualRequest {
            about: "project:kmp".to_string(),
            from: Some(Timestamp {
                seconds: 1,
                nanos: 0,
            }),
            to: Some(Timestamp {
                seconds: 2,
                nanos: 0,
            }),
            level_of_detail: VisualLevelOfDetail::Moment as i32,
            ..Default::default()
        })
        .expect("valid visual query");

        assert_eq!(
            query.level_of_detail,
            kmp_application::VisualLevelOfDetail::Moment
        );
        assert_eq!(query.page_entries, DEFAULT_VISUAL_PAGE);
        assert_eq!(query.from, "unix:100000000001:000000000");
    }
}
