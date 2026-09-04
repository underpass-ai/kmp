use serde_json::{Value, json};

use kmp_proto::v1beta1::{
    CoordinateRelation, CoordinateRelationKind, FactState, ProposedLink, RelateResponse,
    RelatedFact, Tension,
};

use super::rendering::*;

pub(crate) fn relate_from_response(response: RelateResponse) -> Value {
    json!({
        "summary": response.summary,
        "facts": response.facts.iter().map(related_fact_json).collect::<Vec<_>>(),
        "declared": response.declared.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "coordinate": response.coordinate.iter().map(coordinate_relation_json).collect::<Vec<_>>(),
        "tensions": response.tensions.iter().map(tension_json).collect::<Vec<_>>(),
        "proposed": response.proposed.iter().map(proposed_link_json).collect::<Vec<_>>(),
        "proof": response
            .proof
            .as_ref()
            .map(proof_json)
            .unwrap_or_else(empty_proof_json),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "warnings": response.warnings
    })
}

fn related_fact_json(fact: &RelatedFact) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("ref".to_string(), json!(fact.r#ref));
    value.insert("about".to_string(), json!(fact.about));
    value.insert("kind".to_string(), json!(fact.kind));
    value.insert("text".to_string(), json!(fact.text));
    value.insert(
        "coordinates".to_string(),
        json!(
            fact.coordinates
                .iter()
                .map(temporal_coordinate_json)
                .collect::<Vec<_>>()
        ),
    );
    value.insert("state".to_string(), json!(fact_state_label(fact.state)));
    insert_optional_string(&mut value, "superseded_by", &fact.superseded_by);
    insert_optional_timestamp(&mut value, "valid_until", fact.valid_until);
    if !fact.metadata.is_empty() {
        value.insert("metadata".to_string(), json!(fact.metadata));
    }
    Value::Object(value)
}

fn coordinate_relation_json(relation: &CoordinateRelation) -> Value {
    json!({
        "from": relation.from,
        "to": relation.to,
        "kind": coordinate_relation_kind_label(relation.kind),
        "scope_id": relation.scope_id,
        "axis": temporal_axis_label(relation.axis),
        "why": relation.why
    })
}

fn proposed_link_json(link: &ProposedLink) -> Value {
    json!({
        "from": link.from,
        "to": link.to,
        "proposed_by": link.proposed_by,
        "shared": link.shared,
        "idf": link.idf,
        "shared_terms": link.shared_terms,
        "bridged": link.bridged,
        "entities": link.entities,
        "scope_id": link.scope_id,
        "weight": link.weight,
        "why": link.why
    })
}

fn tension_json(tension: &Tension) -> Value {
    json!({
        "ref": tension.r#ref,
        "other": tension.other,
        "scope_id": tension.scope_id,
        "why": tension.why,
        "evidence": tension.evidence
    })
}

fn fact_state_label(value: i32) -> &'static str {
    match FactState::try_from(value) {
        Ok(FactState::Current) => "current",
        Ok(FactState::Superseded) => "superseded",
        Ok(FactState::Expired) => "expired",
        _ => "unknown",
    }
}

fn coordinate_relation_kind_label(value: i32) -> &'static str {
    match CoordinateRelationKind::try_from(value) {
        Ok(CoordinateRelationKind::SharesScope) => "shares_scope",
        Ok(CoordinateRelationKind::Before) => "before",
        Ok(CoordinateRelationKind::After) => "after",
        Ok(CoordinateRelationKind::During) => "during",
        Ok(CoordinateRelationKind::Concurrent) => "concurrent",
        Ok(CoordinateRelationKind::SameSequence) => "same_sequence",
        Ok(CoordinateRelationKind::SameRank) => "same_rank",
        _ => "unknown",
    }
}
