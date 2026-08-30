use serde_json::{Value, json};

use kmp_proto::v1beta1::{ProjectVisualResponse, VisualLevelOfDetail};

use super::rendering::*;

pub(crate) fn visual_projection_from_response(response: ProjectVisualResponse) -> Value {
    json!({
        "contract": response.contract,
        "about": response.about,
        "axis": temporal_axis_label(response.axis),
        "level_of_detail": match VisualLevelOfDetail::try_from(response.level_of_detail) {
            Ok(VisualLevelOfDetail::Episode) => "episode",
            Ok(VisualLevelOfDetail::Moment) => "moment",
            _ => "atlas",
        },
        "range": {
            "from": response.from.map(|value| value.to_string()),
            "to": response.to.map(|value| value.to_string()),
        },
        "bins": response.bins.into_iter().map(|bin| json!({
            "dimension": bin.dimension,
            "from": bin.from.map(|value| value.to_string()),
            "to": bin.to.map(|value| value.to_string()),
            "total": bin.entries,
            "by_kind": bin.by_kind,
        })).collect::<Vec<_>>(),
        "clusters": response.clusters.into_iter().map(|cluster| json!({
            "id": cluster.id,
            "dimension": cluster.dimension,
            "from": cluster.from.map(|value| value.to_string()),
            "to": cluster.to.map(|value| value.to_string()),
            "total": cluster.entries,
            "refs": cluster.refs,
            "by_kind": cluster.by_kind,
        })).collect::<Vec<_>>(),
        "entries": response.entries.iter().map(|entry| json!({
            "ref_id": entry.r#ref,
            "kind": entry.kind,
            "text": entry.text,
            "coordinates": entry.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "by_kind": response.by_kind,
        "relations": response.relations.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "metrics": response.metrics.into_iter().map(|metric| json!({
            "name": metric.name,
            "value": metric.value,
            "unit": metric.unit,
            "scope": metric.scope,
        })).collect::<Vec<_>>(),
        "coverage": response.coverage.map(|coverage| json!({
            "included": coverage.included,
            "missing": coverage.missing,
            "dimensions": coverage.dimensions.iter().map(dimension_coverage_json).collect::<Vec<_>>(),
        })),
        "revision": response.revision,
        "content_hash": response.content_hash,
        "page": response.page.as_ref().map(page_info_json).unwrap_or_else(empty_page_info_json),
        "truncated": response.truncated,
        "missing": response.missing,
    })
}
