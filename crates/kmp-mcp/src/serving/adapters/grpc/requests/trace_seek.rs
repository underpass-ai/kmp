//! JSON boundary for seed-based evidence requests. Shared validation lives in
//! proto mapping so embedded and gRPC callers compile the same obligations.
use super::common::{object, optional_string_array_field, optional_string_field};
use kmp_proto::v1beta1::{
    TraceReferenceEndpoint, TraceRelationStep, TraceSeekOptions, TraceSeekRole, TraceWitnessGroup,
    TraceWitnessLabel,
};
use serde_json::{Map, Value};

/// Reject every cross-mode option together before parsing any option body.
pub(super) fn validate_mode(search: &Map<String, Value>, has_to: bool) -> Result<(), String> {
    if !search.contains_key("seek") {
        return Ok(());
    }
    let mut conflicts = Vec::new();
    if has_to {
        conflicts.push("to".to_owned());
    }
    for key in [
        "follow",
        "direction",
        "relations",
        "dimensions",
        "prefer_dimensions",
        "select",
        "paths_per_target",
    ] {
        if search.contains_key(key) {
            conflicts.push(format!("search.{key}"));
        }
    }
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(format!(
        "search.seek cannot be combined with {}; remove these fields together. Use seek role relations, via/after and witness labels. Only max_nodes, max_edges, max_depth, max_states and proof are shared search options.",
        conflicts.join(", ")
    ))
}

pub(super) fn arguments(search: &Map<String, Value>) -> Result<Option<TraceSeekOptions>, String> {
    let Some(value) = search.get("seek") else {
        if search.contains_key("same_labels") || search.contains_key("same_ref") {
            return Err("search.same_labels and same_ref require seek".into());
        }
        return Ok(None);
    };
    for key in search.keys() {
        if ![
            "seek",
            "same_labels",
            "same_ref",
            "max_nodes",
            "max_edges",
            "max_depth",
            "max_states",
            "proof",
            // Body delivery is shared by both modes: seek materializes the
            // same proof table and pays for the same bodies.
            "max_body_record_bytes",
            "proof_refs",
            "expect_selection",
            "compact",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unsupported search.seek option {key}"));
        }
    }
    let roles = value
        .as_array()
        .filter(|r| !r.is_empty() && r.len() <= 8)
        .ok_or("search.seek requires 1..8 named relations")?
        .iter()
        .map(role)
        .collect::<Result<_, _>>()?;
    let same_ref = match search.get("same_ref") {
        None => vec![],
        Some(Value::Array(groups)) => groups
            .iter()
            .map(|g| {
                let values = g
                    .as_array()
                    .ok_or("search.same_ref requires arrays of role endpoints")?;
                Ok(TraceWitnessGroup {
                    endpoints: values.iter().map(endpoint).collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, String>>()?,
        _ => return Err("search.same_ref requires arrays of role endpoints".into()),
    };
    Ok(Some(TraceSeekOptions {
        roles,
        same_labels: optional_string_array_field(search, "same_labels", "search.same_labels")?,
        same_ref,
    }))
}

fn endpoint(value: &Value) -> Result<TraceReferenceEndpoint, String> {
    if let Some(role) = value.as_str() {
        return Ok(TraceReferenceEndpoint {
            role: role.into(),
            anchor: false,
        });
    }
    let point = object(value, "search.same_ref endpoint")?;
    if point.keys().any(|k| !["role", "at"].contains(&k.as_str())) {
        return Err("search.same_ref endpoint accepts only role and at".into());
    }
    let role = optional_string_field(point, "role", "search.same_ref.role")?
        .ok_or("search.same_ref endpoint requires role")?;
    let at = optional_string_field(point, "at", "search.same_ref.at")?
        .ok_or("search.same_ref endpoint requires at: anchor or witness")?;
    let anchor = match at.as_str() {
        "anchor" => true,
        "witness" => false,
        _ => return Err("search.same_ref.at must be anchor or witness".into()),
    };
    Ok(TraceReferenceEndpoint { role, anchor })
}

fn step(value: &Value) -> Result<TraceRelationStep, String> {
    if let Some(rel) = value.as_str() {
        return Ok(TraceRelationStep {
            rel: rel.into(),
            direction: "outgoing".into(),
        });
    }
    let s = object(value, "search.seek relation")?;
    Ok(TraceRelationStep {
        rel: optional_string_field(s, "rel", "search.seek.rel")?
            .ok_or("search.seek relation requires rel")?,
        direction: optional_string_field(s, "direction", "search.seek.direction")?
            .unwrap_or_else(|| "outgoing".into()),
    })
}

fn role(value: &Value) -> Result<TraceSeekRole, String> {
    let relation = step(value)?;
    let mut role = TraceSeekRole {
        name: relation.rel.clone(),
        relation: Some(relation),
        ..Default::default()
    };
    if let Some(s) = value.as_object() {
        for key in s.keys() {
            if !["name", "rel", "direction", "via", "after", "labels"].contains(&key.as_str()) {
                return Err(format!("unsupported search.seek field {key}"));
            }
        }
        role.name = optional_string_field(s, "name", "search.seek.name")?.unwrap_or(role.name);
        let moves = |key| -> Result<Vec<TraceRelationStep>, String> {
            match s.get(key) {
                None => Ok(vec![]),
                Some(Value::Array(v)) if v.len() < 1024 => v
                    .iter()
                    .map(|v| {
                        if let Some(s) = v.as_object()
                            && s.keys()
                                .any(|k| !["rel", "direction"].contains(&k.as_str()))
                        {
                            return Err(format!(
                                "search.seek.{key} accepts only rel and direction"
                            ));
                        }
                        step(v)
                    })
                    .collect(),
                _ => Err(format!(
                    "search.seek.{key} requires at most 1023 ordered relations"
                )),
            }
        };
        role.context = s.get("via").is_some_and(|v| v == "context");
        role.via = if role.context { vec![] } else { moves("via")? };
        role.after = moves("after")?;
        if let Some(labels) = s.get("labels") {
            role.labels = object(labels, "search.seek.labels")?
                .iter()
                .map(|(key, v)| {
                    let values = v
                        .as_array()
                        .ok_or("search.seek.labels values must be arrays")?
                        .iter()
                        .map(|v| {
                            v.as_str().map(str::to_owned).ok_or_else(|| {
                                "search.seek.labels values must be strings".to_owned()
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    Ok(TraceWitnessLabel {
                        key: key.clone(),
                        values,
                    })
                })
                .collect::<Result<_, String>>()?;
        }
    }
    Ok(role)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn reference_groups_accept_anchor_and_witness_points_without_numeric_positions() {
        let value = json!({"about":"p","from":"s","search":{"seek":[
            {"name":"v","rel":"verified_by","via":"context"},
            {"name":"p","rel":"authorizes","direction":"incoming","via":"context"}],
            "same_ref":[[{"role":"v","at":"anchor"},{"role":"p","at":"anchor"}],["v","p"]]}});
        let request = super::super::queries::trace_request_from_arguments(&value).expect("parsed");
        let native = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
            .expect("valid test fixture")
            .expect("valid test fixture");
        assert_eq!(
            native.roles[0]
                .bindings
                .iter()
                .map(|b| b.at())
                .collect::<Vec<_>>(),
            [0, 1]
        );
        for point in [
            json!({"role":"v","at":"source"}),
            json!({"role":"v"}),
            json!({"role":"v","at":"anchor","extra":true}),
            json!({"role":"absent","at":"anchor"}),
        ] {
            let mut bad = value.clone();
            bad["search"]["same_ref"][0][0] = point;
            let refused = match super::super::queries::trace_request_from_arguments(&bad) {
                Err(_) => true,
                Ok(proto) => {
                    kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&proto).is_err()
                }
            };
            assert!(refused, "{bad}");
        }
    }
    #[test]
    fn shorthand_and_advanced_moves_compile_through_the_same_proto_boundary() {
        let value = json!({"about":"p","from":"s","search":{"seek":["verified_by",{"name":"permission","rel":"authorizes","direction":"incoming","via":["same_entity_as"],"after":[{"rel":"uses_background"}],"labels":{"env":["prod"]}}],"same_labels":["event"]}});
        let request = super::super::queries::trace_request_from_arguments(&value).expect("parsed");
        let native = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
            .expect("valid")
            .expect("seek");
        assert_eq!(native.roles[0].name, "verified_by");
        assert_eq!(native.roles[1].steps.len(), 3);
        assert!(native.roles[1].bindings.iter().all(|b| b.at() == 2));
        assert!(request.to.is_empty());
        for change in [
            json!({"seek":[]}),
            json!({"same_labels":["event"]}),
            json!({"seek":["verified_by"],"dimensions":{}}),
            json!({"seek":[{"rel":"verified_by","lables":{}}]}),
        ] {
            let mut bad = value.clone();
            bad["search"] = change;
            assert!(super::super::queries::trace_request_from_arguments(&bad).is_err());
        }
        let mut bad = value;
        bad["to"] = json!("t");
        assert!(super::super::queries::trace_request_from_arguments(&bad).is_err());
    }
    #[test]
    fn context_discovery_replaces_via_and_does_not_change_the_declared_witness() {
        let value = json!({"about":"p","from":"s","search":{"seek":[{"rel":"verified_by","via":"context","after":["uses_background"],"labels":{"event":["A"]}}]}});
        let request = super::super::queries::trace_request_from_arguments(&value).expect("parsed");
        let native = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
            .expect("valid")
            .expect("seek");
        assert!(native.roles[0].context);
        assert_eq!(native.roles[0].steps.len(), 2);
        assert!(native.roles[0].bindings.iter().all(|b| b.at() == 1));
        let mut bad = value;
        bad["search"]["seek"][0]["via"] = json!("anything");
        assert!(super::super::queries::trace_request_from_arguments(&bad).is_err());
    }
}
