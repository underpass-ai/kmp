//! Compile native relations and witness constraints, with no caller variables.
use super::scalars::{ProtoMappingResult, invalid_argument};
use kmp_domain::{
    EvidencePathBinding, EvidencePathRequest, EvidencePathRole, MemoryRelationType,
    RelationDirection, TraceRelationStep, TraceSearchLimits,
};
use kmp_proto::v1beta1::{TraceRequest, TraceSeekOptions};
use std::collections::{BTreeMap, BTreeSet};

pub fn evidence_seek_request_from_proto(
    request: &TraceRequest,
) -> ProtoMappingResult<Option<EvidencePathRequest>> {
    let Some(options) = request.search.as_ref() else {
        return Ok(None);
    };
    let Some(seek) = options.seek.as_ref() else {
        return Ok(None);
    };
    if !request.to.is_empty()
        || !request.targets.is_empty()
        || !options.follow.is_empty()
        || !options.direction.is_empty()
        || !options.relations.is_empty()
        || options.dimensions.is_some()
        || options.prefer_dimensions.is_some()
        || options.select.is_some()
        || options.paths_per_target != 0
    {
        return Err(invalid_argument(
            "search.seek replaces to/targets and other search policies; only work limits are shared",
        ));
    }
    validate(seek)?;
    let mut roles = Vec::new();
    let mut constants: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (index, role) in seek.roles.iter().enumerate() {
        let main = role
            .relation
            .as_ref()
            .ok_or_else(|| invalid_argument("search.seek role requires rel"))?;
        let steps = role
            .via
            .iter()
            .chain(std::iter::once(main))
            .chain(role.after.iter())
            .map(step)
            .collect::<ProtoMappingResult<Vec<_>>>()?;
        let at = (role.via.len() + 1) as u32;
        let keys: BTreeSet<_> = seek
            .same_labels
            .iter()
            .chain(role.labels.iter().map(|l| &l.key))
            .collect();
        let mut bindings = Vec::new();
        for key in keys {
            let name = if seek.same_labels.contains(key) {
                format!("shared-label:{key}")
            } else {
                format!("role:{index}:label:{key}")
            };
            bindings.push(EvidencePathBinding::Label {
                at,
                name: name.clone(),
                key: key.clone(),
            });
            if let Some(label) = role.labels.iter().find(|l| &l.key == key) {
                let values: BTreeSet<_> = label.values.iter().cloned().collect();
                constants
                    .entry(name)
                    .and_modify(|old| *old = old.intersection(&values).cloned().collect())
                    .or_insert(values);
            }
        }
        for (group, same) in seek.same_ref.iter().enumerate() {
            if same.roles.contains(&role.name) {
                bindings.push(EvidencePathBinding::Reference {
                    at,
                    name: format!("same-ref:{group}"),
                });
            }
        }
        roles.push(EvidencePathRole {
            name: role.name.clone(),
            context: role.context,
            steps,
            bindings,
        });
    }
    if constants.values().any(BTreeSet::is_empty) {
        return Err(invalid_argument(
            "search.seek labels contradict same_labels: allowed witness values have no common value",
        ));
    }
    let defaults = TraceSearchLimits::default();
    let or_default = |value, default| if value == 0 { default } else { value };
    let result = EvidencePathRequest {
        about: request.about.clone(),
        from: request.from.clone(),
        roles,
        constants,
        temporal: super::queries::temporal_selection_from_proto(
            request.as_of.clone(),
            request.interval,
            request.axis,
        )?,
        limits: TraceSearchLimits {
            nodes: or_default(options.max_nodes, defaults.nodes),
            edges: or_default(options.max_edges, defaults.edges),
            depth: or_default(options.max_depth, defaults.depth),
            states: or_default(options.max_states, defaults.states),
        },
    };
    result
        .validate()
        .map_err(|e| invalid_argument(e.to_string()))?;
    Ok(Some(result))
}

fn step(step: &kmp_proto::v1beta1::TraceRelationStep) -> ProtoMappingResult<TraceRelationStep> {
    let relation =
        MemoryRelationType::new(step.rel.clone()).map_err(|e| invalid_argument(e.to_string()))?;
    if relation.is_structural() {
        return Err(invalid_argument(
            "search.seek requires a non-structural relation with why and evidence",
        ));
    }
    let direction = match step.direction.as_str() {
        "" | "outgoing" => RelationDirection::Outgoing,
        "incoming" => RelationDirection::Incoming,
        _ => {
            return Err(invalid_argument(
                "search.seek direction must be outgoing or incoming",
            ));
        }
    };
    Ok(TraceRelationStep {
        relation,
        direction,
    })
}

fn validate(seek: &TraceSeekOptions) -> ProtoMappingResult<()> {
    let distinct = |values: &[String], max: usize| {
        values.len() <= max
            && values.iter().all(|s| !s.trim().is_empty())
            && values.iter().collect::<BTreeSet<_>>().len() == values.len()
    };
    if seek.roles.is_empty()
        || seek.roles.len() > 8
        || !distinct(
            &seek
                .roles
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            8,
        )
    {
        return Err(invalid_argument(
            "search.seek requires 1..8 unique nonempty role names; set name when repeating a relation",
        ));
    }
    if !distinct(&seek.same_labels, 16) {
        return Err(invalid_argument(
            "search.same_labels requires at most 16 distinct nonempty label keys",
        ));
    }
    let mut joined = BTreeSet::new();
    for same in &seek.same_ref {
        if same.roles.len() < 2
            || !distinct(&same.roles, 8)
            || same
                .roles
                .iter()
                .any(|name| !seek.roles.iter().any(|r| &r.name == name) || !joined.insert(name))
        {
            return Err(invalid_argument(
                "search.same_ref needs disjoint groups of 2..8 existing role names; put all equal witnesses in one group",
            ));
        }
    }
    for role in &seek.roles {
        if role.context && !role.via.is_empty() {
            return Err(invalid_argument(
                "search.seek context discovery replaces explicit via moves",
            ));
        }
        if !distinct(
            &role
                .labels
                .iter()
                .map(|l| l.key.clone())
                .collect::<Vec<_>>(),
            16,
        ) || role
            .labels
            .iter()
            .any(|l| l.values.is_empty() || !distinct(&l.values, 64))
        {
            return Err(invalid_argument(
                "search.seek.labels needs at most 16 distinct keys, each with 1..64 distinct nonempty values",
            ));
        }
        if role.via.len().saturating_add(role.after.len()) >= 1024 {
            return Err(invalid_argument(
                "search.seek role permits at most 1024 total ordered steps",
            ));
        }
    }
    Ok(())
}
