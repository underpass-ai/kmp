use serde_json::{Map, Value, json};

use kmp_proto::v1beta1::{ProjectVisualResponse, VisualLabel, VisualLevelOfDetail};

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
            "scope_id": bin.scope_id,
            "from": bin.from.map(|value| value.to_string()),
            "to": bin.to.map(|value| value.to_string()),
            "total": bin.entries,
            "by_kind": bin.by_kind,
        })).collect::<Vec<_>>(),
        "clusters": response.clusters.into_iter().map(|cluster| json!({
            "id": cluster.id,
            "dimension": cluster.dimension,
            "scope_id": cluster.scope_id,
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
        "labels": response.labels.iter().map(visual_label_json).collect::<Vec<_>>(),
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

/// One row of the catalogue as the loom reads it. `last_observed_at` is
/// left out when the label never carried a clock rather than written null.
fn visual_label_json(label: &VisualLabel) -> Value {
    let mut object = Map::new();
    object.insert("dimension".to_string(), json!(label.dimension));
    object.insert("scope_id".to_string(), json!(label.scope_id));
    object.insert("value".to_string(), json!(label.value));
    object.insert("in_range".to_string(), json!(label.in_range));
    object.insert("entries".to_string(), json!(label.entries));
    insert_optional_timestamp(&mut object, "last_observed_at", label.last_observed_at);
    Value::Object(object)
}
