use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{
    MemoryRelationQuality, MemoryRelationSpec, MemoryRelationType, RelationSemanticClass,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const DEFAULT_CONFIDENCE: &str = "high";
const DEFAULT_SOURCE_KIND: &str = "agent";
const NON_STRUCTURAL_RELATION_CLASSES: &[RelationSemanticClass] = &[
    RelationSemanticClass::Causal,
    RelationSemanticClass::Motivational,
    RelationSemanticClass::Procedural,
    RelationSemanticClass::Evidential,
    RelationSemanticClass::Constraint,
];

#[derive(Clone, Copy, Debug)]
struct ResolvedRelationSpec {
    quality: MemoryRelationQuality,
    classes: &'static [RelationSemanticClass],
    reason: &'static str,
}

impl From<MemoryRelationSpec> for ResolvedRelationSpec {
    fn from(value: MemoryRelationSpec) -> Self {
        Self {
            quality: value.quality(),
            classes: value.allowed_classes(),
            reason: value.reason(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KernelWritePlan {
    pub(crate) about: String,
    pub(crate) dry_run: bool,
    pub(crate) ingest_arguments: Value,
    pub(crate) generated_refs: Vec<String>,
    pub(crate) relations: Vec<String>,
    pub(crate) relation_quality: Vec<Value>,
    pub(crate) relation_quality_metrics: Value,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) next_suggested_reads: Vec<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReadContext {
    ref_sources: BTreeMap<String, BTreeSet<String>>,
}

impl ReadContext {
    fn from_arguments(arguments: &Map<String, Value>) -> Result<Self, String> {
        let Some(read_context) = arguments.get("read_context") else {
            return Ok(Self::default());
        };
        let read_context = read_context
            .as_object()
            .ok_or_else(|| "argument `read_context` must be an object".to_string())?;
        let mut context = Self::default();
        context.collect_ref_array(read_context, "inspected_refs", "kernel_inspect")?;
        context.collect_ref_array(read_context, "temporal_refs", "kernel_temporal")?;
        context.collect_ref_array(read_context, "wake_refs", "kernel_wake")?;
        context.collect_ref_array(read_context, "ask_refs", "kernel_ask")?;
        context.collect_trace_paths(read_context)?;
        Ok(context)
    }

    fn add_ref(&mut self, ref_id: &str, source: &str) {
        if ref_id.trim().is_empty() {
            return;
        }
        self.ref_sources
            .entry(ref_id.to_string())
            .or_default()
            .insert(source.to_string());
    }

    fn sources_for(&self, ref_id: &str) -> Vec<String> {
        self.ref_sources
            .get(ref_id)
            .map(|sources| sources.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn collect_ref_array(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        source: &str,
    ) -> Result<(), String> {
        for (index, ref_value) in optional_array(object.get(key), &format!("read_context.{key}"))?
            .iter()
            .enumerate()
        {
            let ref_id = ref_value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("read_context.{key}[{index}] must be a non-empty string"))?;
            self.add_ref(ref_id, source);
        }
        Ok(())
    }

    fn collect_trace_paths(&mut self, object: &Map<String, Value>) -> Result<(), String> {
        for (index, trace) in optional_array(object.get("trace_paths"), "read_context.trace_paths")?
            .iter()
            .enumerate()
        {
            let trace = trace
                .as_object()
                .ok_or_else(|| format!("read_context.trace_paths[{index}] must be an object"))?;
            let from = required_map_string(
                trace,
                "from",
                &format!("read_context.trace_paths[{index}].from"),
            )?;
            let to = required_map_string(
                trace,
                "to",
                &format!("read_context.trace_paths[{index}].to"),
            )?;
            self.add_ref(from, "kernel_trace");
            self.add_ref(to, "kernel_trace");
            for (ref_index, ref_value) in optional_array(
                trace.get("refs"),
                &format!("read_context.trace_paths[{index}].refs"),
            )?
            .iter()
            .enumerate()
            {
                let ref_id = ref_value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        format!(
                            "read_context.trace_paths[{index}].refs[{ref_index}] must be a non-empty string"
                        )
                    })?;
                self.add_ref(ref_id, "kernel_trace");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn build_write_plan(arguments: &Value) -> Result<KernelWritePlan, String> {
    build_write_plan_with_root(arguments, false)
}

/// Builds a write plan, allowing the one relation-free strict write that can
/// form a new about's root. The server proves `allow_unlinked_root` by
/// inspecting the about immediately before calling this function; keeping
/// the storage read outside the pure compiler preserves deterministic dry
/// runs and focused validation tests.
pub(crate) fn build_write_plan_with_root(
    arguments: &Value,
    allow_unlinked_root: bool,
) -> Result<KernelWritePlan, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    let intent = required_string(arguments, "intent")?;
    validate_intent(&intent)?;
    let actor = required_string(arguments, "actor")?;
    let observed_at = required_string(arguments, "observed_at")?;
    let scope = required_object(arguments, "scope")?;
    let read_context = ReadContext::from_arguments(arguments)?;
    let process_scope = required_map_string(scope, "process", "scope.process")?;
    let task_scope = optional_map_string(scope, "task");
    let episode_scope = optional_map_string(scope, "episode");
    let options = arguments.get("options").and_then(Value::as_object);
    // A tool called write_memory commits. Previewing was the default here,
    // so every caller that did not know to pass `dry_run: false` got
    // `isError: false` back and wrote nothing — the skill and the write
    // protocol doc both describe the opposite, and `accepted: false` is easy
    // to miss in a tool result. Opt in to the preview, not out of it.
    let dry_run = options
        .and_then(|options| options.get("dry_run"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let strict = options
        .and_then(|options| options.get("strict"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let sequence = options
        .and_then(|options| options.get("sequence"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);

    let current = required_object(arguments, "current")?;
    let current_kind = required_map_string(current, "kind", "current.kind")?;
    validate_node_kind(current_kind)?;
    let current_summary = required_map_string(current, "summary", "current.summary")?;
    let current_evidence = optional_map_string(current, "evidence");
    if strict && current_evidence.is_none() {
        return Err("strict kernel_write_memory requires current.evidence".to_string());
    }

    let current_ref = optional_map_string(current, "ref")
        .map(ToString::to_string)
        .unwrap_or_else(|| generated_entry_ref(&about, current_kind, current_summary));
    let mut generated_refs = vec![current_ref.clone()];
    let mut local_refs = BTreeSet::from([current_ref.clone()]);
    let mut dimensions = Vec::new();
    let mut coordinates = Vec::new();
    if let Some(task_scope) = task_scope {
        dimensions.push(dimension(task_scope, "task", "Kernel write task"));
        coordinates.push(coordinate("task", task_scope, sequence, &observed_at));
    }
    dimensions.push(dimension(
        process_scope,
        "agentic_process",
        "Kernel write process",
    ));
    coordinates.push(coordinate(
        "agentic_process",
        process_scope,
        sequence,
        &observed_at,
    ));
    if let Some(episode_scope) = episode_scope {
        dimensions.push(dimension(
            episode_scope,
            "agentic_episode",
            "Kernel write episode",
        ));
        coordinates.push(coordinate(
            "agentic_episode",
            episode_scope,
            sequence,
            &observed_at,
        ));
    }

    let mut entries = vec![json!({
        "id": current_ref.clone(),
        "kind": current_kind,
        "text": current_summary,
        "coordinates": coordinates.clone(),
        "metadata": {
            "writer_intent": intent,
            "writer_actor": actor
        }
    })];
    let mut relations = Vec::new();
    let mut relation_names = Vec::new();
    let mut relation_quality = Vec::new();
    let mut evidence = Vec::new();
    if let Some(current_evidence) = current_evidence {
        evidence.push(json!({
            "id": format!("evidence:{}:current", current_ref),
            "supports": [current_ref.clone()],
            "text": current_evidence,
            "source": format!("kernel_write_memory:{actor}"),
            "time": observed_at
        }));
    }

    let connect_to = optional_array(arguments.get("connect_to"), "connect_to")?;
    if strict && connect_to.is_empty() && !allow_unlinked_root {
        return Err(
            "strict kernel_write_memory requires at least one connect_to relation once the about exists; inspect or traverse a target first, or set options.strict=false when an unlinked write is intentional"
                .to_string(),
        );
    }
    for (index, link) in connect_to.iter().enumerate() {
        let link = link
            .as_object()
            .ok_or_else(|| format!("connect_to[{index}] must be an object"))?;
        let target_ref = required_map_string(link, "ref", &format!("connect_to[{index}].ref"))?;
        let rel_arg = required_map_string(link, "rel", &format!("connect_to[{index}].rel"))?;
        let relation_type = MemoryRelationType::new(rel_arg)
            .map_err(|error| format!("connect_to[{index}].rel is invalid: {error}"))?;
        let rel = relation_type.as_str();
        let semantic_class =
            required_map_string(link, "class", &format!("connect_to[{index}].class"))?;
        validate_semantic_class(semantic_class)?;
        let why = required_relation_string(link, "why", semantic_class, index)?;
        let relation_evidence = required_relation_string(link, "evidence", semantic_class, index)?;
        let confidence = optional_map_string(link, "confidence").unwrap_or(DEFAULT_CONFIDENCE);
        validate_confidence(confidence)?;
        let quality = relation_quality_diagnostic(RelationQualityInput {
            from: &current_ref,
            to: target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            evidence: relation_evidence,
            strict,
            read_context: &read_context,
            local_refs: &local_refs,
        })?;

        relations.push(relation(
            &current_ref,
            target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            relation_evidence,
            sequence,
        ));
        relation_names.push(rel.to_string());
        relation_quality.push(quality);
        // A structural link is exempt from evidence, and an evidence item with
        // no text is not evidence: the canonical ingest mapper requires
        // `memory.evidence[].text`, and rightly refuses an empty one.
        if !relation_evidence.trim().is_empty() {
            evidence.push(json!({
                "id": format!("evidence:{}:relation:{}", current_ref, index + 1),
                "supports": [current_ref.clone(), target_ref],
                "text": relation_evidence,
                "source": format!("kernel_write_memory:{actor}:relation:{rel}"),
                "time": observed_at
            }));
        }
    }

    if let Some(delta) = arguments.get("semantic_delta").and_then(Value::as_object) {
        let delta_from = required_map_string(delta, "from", "semantic_delta.from")?;
        let delta_to = required_map_string(delta, "to", "semantic_delta.to")?;
        let delta_why = required_map_string(delta, "why", "semantic_delta.why")?;
        let delta_evidence = required_map_string(delta, "evidence", "semantic_delta.evidence")?;
        let delta_ref = optional_map_string(delta, "ref")
            .map(ToString::to_string)
            .unwrap_or_else(|| generated_entry_ref(&about, "semantic_delta", delta_to));
        reject_duplicate_ref(&mut generated_refs, &delta_ref)?;
        local_refs.insert(delta_ref.clone());
        entries.push(json!({
            "id": delta_ref.clone(),
            "kind": "semantic_delta",
            "text": format!("From: {delta_from}\nTo: {delta_to}\nWhy: {delta_why}"),
            "coordinates": shifted_coordinates(&entries[0]["coordinates"], 1),
            "metadata": {
                "writer_intent": intent,
                "writer_actor": actor,
                "delta_from": delta_from,
                "delta_to": delta_to
            }
        }));
        let updates_state_quality = relation_quality_diagnostic(RelationQualityInput {
            from: &current_ref,
            to: &delta_ref,
            rel: "updates_state",
            semantic_class: "causal",
            confidence: DEFAULT_CONFIDENCE,
            why: delta_why,
            evidence: delta_evidence,
            strict,
            read_context: &read_context,
            local_refs: &local_refs,
        })?;
        relations.push(relation(
            &current_ref,
            &delta_ref,
            "updates_state",
            "causal",
            DEFAULT_CONFIDENCE,
            delta_why,
            delta_evidence,
            sequence + 1,
        ));
        relation_names.push("updates_state".to_string());
        relation_quality.push(updates_state_quality);
        if let Some(first_link) = connect_to.first().and_then(Value::as_object) {
            let target_ref = required_map_string(first_link, "ref", "connect_to[0].ref")?;
            let semantic_delta_quality = relation_quality_diagnostic(RelationQualityInput {
                from: &delta_ref,
                to: target_ref,
                rel: "semantic_delta_from",
                semantic_class: "causal",
                confidence: DEFAULT_CONFIDENCE,
                why: delta_why,
                evidence: delta_evidence,
                strict,
                read_context: &read_context,
                local_refs: &local_refs,
            })?;
            relations.push(relation(
                &delta_ref,
                target_ref,
                "semantic_delta_from",
                "causal",
                DEFAULT_CONFIDENCE,
                delta_why,
                delta_evidence,
                sequence + 1,
            ));
            relation_names.push("semantic_delta_from".to_string());
            relation_quality.push(semantic_delta_quality);
        }
        evidence.push(json!({
            "id": format!("evidence:{}:semantic_delta", delta_ref),
            "supports": [delta_ref.clone(), current_ref.clone()],
            "text": delta_evidence,
            "source": format!("kernel_write_memory:{actor}:semantic_delta"),
            "time": observed_at
        }));
    }

    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_idempotency_key(arguments));
    let ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": idempotency_key.clone(),
        "dry_run": dry_run,
        "memory": {
            "dimensions": dimensions,
            "entries": entries,
            "relations": relations,
            "evidence": evidence
        },
        "provenance": {
            "source_kind": arguments
                .get("source_kind")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SOURCE_KIND),
            "source_agent": actor,
            "observed_at": observed_at,
            "correlation_id": format!("kernel_write:{about}"),
            "causation_id": idempotency_key
        }
    });
    let relation_quality_metrics = relation_quality_metrics(&relation_quality);

    Ok(KernelWritePlan {
        about,
        dry_run,
        ingest_arguments,
        generated_refs,
        relations: relation_names,
        relation_quality,
        relation_quality_metrics,
        diagnostics: Vec::new(),
        next_suggested_reads: suggested_reads(&current_ref, connect_to),
    })
}

pub(crate) fn write_dry_run_result(plan: &KernelWritePlan) -> Value {
    json!({
        "accepted": false,
        "dry_run": true,
        "summary": write_summary(plan),
        "generated_refs": plan.generated_refs,
        "relations": plan.relations,
        "relation_quality": plan.relation_quality,
        "relation_quality_metrics": plan.relation_quality_metrics,
        "ingest_preview": plan.ingest_arguments,
        "diagnostics": plan.diagnostics,
        "next_suggested_reads": plan.next_suggested_reads
    })
}

pub(crate) fn write_commit_result(plan: &KernelWritePlan, ingest_result: Value) -> Value {
    json!({
        "accepted": true,
        "dry_run": false,
        "summary": write_summary(plan),
        "generated_refs": plan.generated_refs,
        "relations": plan.relations,
        "relation_quality": plan.relation_quality,
        "relation_quality_metrics": plan.relation_quality_metrics,
        "ingest_result": ingest_result,
        "diagnostics": plan.diagnostics,
        "next_suggested_reads": plan.next_suggested_reads
    })
}

fn write_summary(plan: &KernelWritePlan) -> String {
    let memory = &plan.ingest_arguments["memory"];
    let entry_count = memory["entries"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let relation_count = memory["relations"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let evidence_count = memory["evidence"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    format!(
        "Prepared {entry_count} {}, {relation_count} {}, and {evidence_count} {} for {}.",
        plural(entry_count, "entry", "entries"),
        plural(relation_count, "relation", "relations"),
        plural(evidence_count, "evidence item", "evidence items"),
        plan.about
    )
}

fn dimension(id: &str, kind: &str, title: &str) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "title": title
    })
}

fn coordinate(dimension: &str, scope_id: &str, sequence: u32, observed_at: &str) -> Value {
    json!({
        "dimension": dimension,
        "scope_id": scope_id,
        "sequence": sequence,
        "observed_at": observed_at
    })
}

fn shifted_coordinates(coordinates: &Value, offset: u32) -> Value {
    let mut shifted = coordinates.clone();
    if let Some(coordinates) = shifted.as_array_mut() {
        for coordinate in coordinates {
            if let Some(sequence) = coordinate.get_mut("sequence") {
                *sequence = json!(sequence.as_u64().unwrap_or_default() + u64::from(offset));
            }
        }
    }
    shifted
}

#[allow(clippy::too_many_arguments)]
fn relation(
    from: &str,
    to: &str,
    rel: &str,
    semantic_class: &str,
    confidence: &str,
    why: &str,
    evidence: &str,
    sequence: u32,
) -> Value {
    // A structural link carries no rationale, and that is the writer's own
    // rule: `why` and `evidence` are required for every other class. The
    // canonical ingest mapper reads a *present but empty* string as a
    // malformed argument rather than an absent one, so emitting `""` here
    // rejected exactly the writes the rule allows. Omit the keys instead.
    let mut relation = json!({
        "from": from,
        "to": to,
        "rel": rel,
        "class": semantic_class,
        "confidence": confidence,
        "sequence": sequence
    });
    let fields = relation
        .as_object_mut()
        .expect("relation literal is a JSON object");
    if !why.trim().is_empty() {
        fields.insert("why".to_string(), json!(why));
    }
    if !evidence.trim().is_empty() {
        fields.insert("evidence".to_string(), json!(evidence));
    }
    relation
}

fn suggested_reads(current_ref: &str, connect_to: &[Value]) -> Vec<Value> {
    connect_to
        .first()
        .and_then(Value::as_object)
        .and_then(|link| link.get("ref"))
        .and_then(Value::as_str)
        .map(|target_ref| {
            vec![json!({
                "tool": "kernel_trace",
                "from": current_ref,
                "to": target_ref
            })]
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug)]
struct RelationQualityInput<'a> {
    from: &'a str,
    to: &'a str,
    rel: &'a str,
    semantic_class: &'a str,
    confidence: &'a str,
    why: &'a str,
    evidence: &'a str,
    strict: bool,
    read_context: &'a ReadContext,
    local_refs: &'a BTreeSet<String>,
}

fn relation_quality_diagnostic(input: RelationQualityInput<'_>) -> Result<Value, String> {
    let spec = relation_spec(input.rel, input.strict)?;
    let semantic_class = RelationSemanticClass::parse(input.semantic_class)
        .map_err(|error| format!("kernel_write_memory relation class is invalid: {error}"))?;
    if !spec.classes.contains(&semantic_class) {
        return Err(format!(
            "kernel_write_memory relation `{}` cannot use class `{}`; expected one of {}",
            input.rel,
            input.semantic_class,
            class_names(spec.classes).join(", ")
        ));
    }

    let target_present = !input.to.trim().is_empty();
    let proof_complete = input.semantic_class == "structural"
        || (!input.why.trim().is_empty() && !input.evidence.trim().is_empty());
    if !target_present {
        return Err(format!(
            "kernel_write_memory relation `{}` requires a target ref",
            input.rel
        ));
    }
    if input.from == input.to {
        return Err(format!(
            "kernel_write_memory relation `{}` cannot point from and to the same ref `{}`",
            input.rel, input.from
        ));
    }
    if input.strict && input.semantic_class != "structural" && !proof_complete {
        return Err(format!(
            "kernel_write_memory relation `{}` requires both why and evidence in strict mode",
            input.rel
        ));
    }

    let target_is_local = input.local_refs.contains(input.to);
    let prior_context_sources = if target_is_local {
        vec!["current_request".to_string()]
    } else {
        input.read_context.sources_for(input.to)
    };
    let prior_context_observed = !prior_context_sources.is_empty();
    if input.strict
        && spec.quality == MemoryRelationQuality::Rich
        && !target_is_local
        && !prior_context_observed
    {
        return Err(format!(
            "strict kernel_write_memory rich relation `{}` to `{}` requires read_context evidence; inspect, trace, or traverse the target first, or use an explicit anemic fallback",
            input.rel, input.to
        ));
    }

    let quality = if !input.strict
        && spec.quality == MemoryRelationQuality::Rich
        && (!prior_context_observed || matches!(input.confidence, "low" | "unknown"))
    {
        MemoryRelationQuality::Suspect
    } else {
        spec.quality
    };
    let requires_prior_context = spec.quality == MemoryRelationQuality::Rich && !target_is_local;

    Ok(json!({
        "from": input.from,
        "to": input.to,
        "rel": input.rel,
        "class": input.semantic_class,
        "confidence": input.confidence,
        "quality": quality.as_str(),
        "quality_reason": relation_quality_reason(quality, spec.reason),
        "fallback": quality == MemoryRelationQuality::Anemic,
        "requires_prior_context": requires_prior_context,
        "prior_context_observed": prior_context_observed,
        "prior_context_sources": prior_context_sources,
        "proof_complete": proof_complete,
        "target_present": target_present
    }))
}

fn relation_spec(rel: &str, strict: bool) -> Result<ResolvedRelationSpec, String> {
    let relation_type = MemoryRelationType::new(rel)
        .map_err(|error| format!("kernel_write_memory relation type is invalid: {error}"))?;
    if let Some(spec) = relation_type.writer_spec() {
        return Ok(spec.into());
    }
    if !strict {
        return Ok(ResolvedRelationSpec {
            quality: MemoryRelationQuality::Suspect,
            classes: NON_STRUCTURAL_RELATION_CLASSES,
            reason: "non-strict relation is outside the canonical writer vocabulary",
        });
    }
    Err(format!(
        "unsupported or vague kernel_write_memory relation `{rel}`"
    ))
}

fn relation_quality_reason(quality: MemoryRelationQuality, default_reason: &str) -> &str {
    match quality {
        MemoryRelationQuality::Rich => {
            "non-structural relation has target ref, why, evidence, and supported semantic class"
        }
        MemoryRelationQuality::Anemic | MemoryRelationQuality::Structural => default_reason,
        MemoryRelationQuality::Suspect => {
            "relation was accepted only because strict mode is disabled and must be audited"
        }
    }
}

fn class_names(classes: &[RelationSemanticClass]) -> Vec<&'static str> {
    classes.iter().map(RelationSemanticClass::as_str).collect()
}

fn relation_quality_metrics(relation_quality: &[Value]) -> Value {
    let relation_total = relation_quality.len();
    let relation_rich_count = quality_count(relation_quality, "rich");
    let relation_anemic_count = quality_count(relation_quality, "anemic");
    let relation_structural_count = quality_count(relation_quality, "structural");
    let relation_suspect_count = quality_count(relation_quality, "suspect");
    let semantic_total = relation_rich_count + relation_anemic_count + relation_suspect_count;
    let proof_complete = relation_quality
        .iter()
        .filter(|relation| relation["proof_complete"].as_bool().unwrap_or(false))
        .count();
    let target_present = relation_quality
        .iter()
        .filter(|relation| relation["target_present"].as_bool().unwrap_or(false))
        .count();
    let prior_context_required = relation_quality
        .iter()
        .filter(|relation| {
            relation["requires_prior_context"]
                .as_bool()
                .unwrap_or(false)
        })
        .count();
    let prior_context_observed = relation_quality
        .iter()
        .filter(|relation| {
            relation["requires_prior_context"]
                .as_bool()
                .unwrap_or(false)
                && relation["prior_context_observed"]
                    .as_bool()
                    .unwrap_or(false)
        })
        .count();
    let non_structural = relation_total.saturating_sub(relation_structural_count);
    let explanatory = relation_quality
        .iter()
        .filter(|relation| {
            matches!(
                relation["class"].as_str(),
                Some("causal" | "motivational" | "evidential" | "constraint")
            )
        })
        .count();

    json!({
        "relation_total": relation_total,
        "relation_rich_count": relation_rich_count,
        "relation_anemic_count": relation_anemic_count,
        "relation_structural_count": relation_structural_count,
        "relation_invalid_rejected_count": 0,
        "relation_suspect_count": relation_suspect_count,
        "relation_rich_ratio": ratio(relation_rich_count, semantic_total),
        "relation_anemic_ratio": ratio(relation_anemic_count, semantic_total),
        "relation_explanatory_ratio": ratio(explanatory, non_structural),
        "relation_proof_coverage": ratio(proof_complete, relation_total),
        "relation_target_coverage": ratio(target_present, relation_total),
        "relation_prior_context_required_count": prior_context_required,
        "relation_prior_context_observed_count": prior_context_observed,
        "relation_prior_context_coverage": ratio(prior_context_observed, prior_context_required)
    })
}

fn quality_count(relation_quality: &[Value], quality: &str) -> usize {
    relation_quality
        .iter()
        .filter(|relation| relation["quality"].as_str() == Some(quality))
        .count()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn required_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, String> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing required object argument `{key}`"))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

fn required_map_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{path}`"))
}

fn optional_map_string<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn optional_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn optional_array<'a>(value: Option<&'a Value>, key: &str) -> Result<&'a [Value], String> {
    match value {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("argument `{key}` must be an array")),
        None => Ok(&[]),
    }
}

fn required_relation_string<'a>(
    relation: &'a Map<String, Value>,
    key: &str,
    semantic_class: &str,
    index: usize,
) -> Result<&'a str, String> {
    if semantic_class == "structural" {
        return Ok(optional_map_string(relation, key).unwrap_or(""));
    }
    required_map_string(relation, key, &format!("connect_to[{index}].{key}"))
}

fn reject_duplicate_ref(refs: &mut Vec<String>, new_ref: &str) -> Result<(), String> {
    if refs.iter().any(|existing| existing == new_ref) {
        return Err(format!("generated duplicate memory ref `{new_ref}`"));
    }
    refs.push(new_ref.to_string());
    Ok(())
}

fn validate_intent(value: &str) -> Result<(), String> {
    match value {
        "record_turn" | "record_observation" | "record_decision" | "record_feedback"
        | "record_delta" => Ok(()),
        other => Err(format!("invalid kernel_write_memory intent `{other}`")),
    }
}

fn validate_node_kind(value: &str) -> Result<(), String> {
    match value {
        "turn" | "observation" | "decision" | "feedback" | "semantic_delta" | "constraint"
        | "preference" | "derived_value" | "error_path" | "success_path" => Ok(()),
        other => Err(format!(
            "invalid kernel_write_memory current.kind `{other}`"
        )),
    }
}

fn validate_semantic_class(value: &str) -> Result<(), String> {
    match value {
        "structural" | "causal" | "motivational" | "procedural" | "evidential" | "constraint" => {
            Ok(())
        }
        other => Err(format!(
            "invalid kernel_write_memory relation class `{other}`"
        )),
    }
}

fn validate_confidence(value: &str) -> Result<(), String> {
    match value {
        "high" | "medium" | "low" | "unknown" => Ok(()),
        other => Err(format!(
            "invalid kernel_write_memory relation confidence `{other}`"
        )),
    }
}

fn generated_entry_ref(about: &str, kind: &str, summary: &str) -> String {
    let slug = sanitize_ref_segment(summary);
    let suffix = if slug.is_empty() {
        short_hash(summary)
    } else {
        slug
    };
    format!("{about}:entry:{kind}:{suffix}")
}

fn stable_idempotency_key(arguments: &Map<String, Value>) -> String {
    let mut stable = Value::Object(arguments.clone());
    if let Some(options) = stable.get_mut("options").and_then(Value::as_object_mut) {
        options.remove("dry_run");
    }
    format!("write:{}", short_hash(&stable.to_string()))
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}").chars().take(16).collect()
}

fn sanitize_ref_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous_was_separator = false;
    for ch in input.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };

        if normalized == '-' {
            if !previous_was_separator {
                output.push(normalized);
            }
            previous_was_separator = true;
        } else {
            output.push(normalized);
            previous_was_separator = false;
        }
        if output.len() >= 80 {
            break;
        }
    }
    output.trim_matches('-').to_string()
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn write_commits_unless_a_preview_is_requested() {
        // No `options` at all is the shape a caller reaches for first, and it
        // must commit: previewing by default meant `isError: false` on a call
        // that wrote nothing.
        let mut request = sample_write_request();
        request.as_object_mut().expect("object").remove("options");
        let plan = build_write_plan(&request).expect("write should plan");
        assert!(
            !plan.dry_run,
            "write_memory must commit by default; previewing is opt-in"
        );

        let mut strict_only = sample_write_request();
        strict_only["options"] = json!({ "strict": true });
        let plan = build_write_plan(&strict_only).expect("write should plan");
        assert!(
            !plan.dry_run,
            "setting an unrelated option must not turn a write into a preview"
        );
    }

    #[test]
    fn dry_run_generates_canonical_ingest_preview() {
        let plan = build_write_plan(&sample_write_request()).expect("write should plan");

        assert!(plan.dry_run);
        assert_eq!(plan.generated_refs.len(), 2);
        assert_eq!(
            plan.relations,
            vec!["chosen_because", "updates_state", "semantic_delta_from"]
        );
        assert_eq!(plan.relation_quality.len(), 3);
        assert_eq!(
            plan.relation_quality_metrics["relation_rich_count"],
            json!(3)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_anemic_count"],
            json!(0)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_prior_context_required_count"],
            json!(2)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_prior_context_coverage"],
            json!(1.0)
        );
        assert_eq!(plan.relation_quality[0]["quality"], json!("rich"));
        assert_eq!(
            plan.relation_quality[0]["prior_context_sources"],
            json!(["kernel_inspect"])
        );
        assert_eq!(plan.ingest_arguments["about"], "incident:mobile-login");
        assert_eq!(
            plan.ingest_arguments["memory"]["dimensions"][0]["kind"],
            "task"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["dimensions"][1]["kind"],
            "agentic_process"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["entries"][1]["kind"],
            "semantic_delta"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][0]["rel"],
            "chosen_because"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][2]["rel"],
            "semantic_delta_from"
        );
        assert_eq!(
            plan.ingest_arguments["provenance"]["source_agent"],
            "agent:backend"
        );
    }

    #[test]
    fn structural_links_compile_without_an_empty_rationale() {
        // The first memory in a store is often a plain `scoped_to` into its
        // own about, with no rationale to give — structural links are exempt
        // from why and evidence by this tool's own contract. Compiling that
        // exemption to `"why": ""` made the canonical ingest mapper reject the
        // write as a malformed argument, so the exemption existed on paper
        // only.
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("semantic_delta");
        request["connect_to"] = json!([{
            "ref": "incident:mobile-login",
            "rel": "scoped_to",
            "class": "structural"
        }]);

        let plan = build_write_plan(&request).expect("a structural link needs no rationale");

        let relation = &plan.ingest_arguments["memory"]["relations"][0];
        assert_eq!(relation["rel"], "scoped_to");
        assert!(
            relation.get("why").is_none(),
            "an absent why must stay absent, not become an empty string: {relation}"
        );
        assert!(
            relation.get("evidence").is_none(),
            "an absent evidence must stay absent, not become an empty string: {relation}"
        );

        let evidence = plan.ingest_arguments["memory"]["evidence"]
            .as_array()
            .expect("evidence should be an array");
        assert_eq!(
            evidence.len(),
            1,
            "only the entry's own evidence survives; a structural link contributes none: {evidence:?}"
        );
        assert_eq!(
            evidence[0]["text"],
            "Logs show 401 immediately after token refresh."
        );
    }

    #[test]
    fn stable_idempotency_ignores_dry_run_switch() {
        let mut commit = sample_write_request();
        commit["options"]["dry_run"] = json!(false);

        let dry_plan = build_write_plan(&sample_write_request()).expect("dry run should plan");
        let commit_plan = build_write_plan(&commit).expect("commit should plan");

        assert_eq!(
            dry_plan.ingest_arguments["idempotency_key"],
            commit_plan.ingest_arguments["idempotency_key"]
        );
        assert_ne!(
            dry_plan.ingest_arguments["dry_run"],
            commit_plan.ingest_arguments["dry_run"]
        );
    }

    #[test]
    fn rejects_missing_process_scope() {
        let mut request = sample_write_request();
        request["scope"]
            .as_object_mut()
            .expect("sample scope should be an object")
            .remove("process");

        let error = build_write_plan(&request).expect_err("process scope is required");

        assert_eq!(error, "missing required argument `scope.process`");
    }

    #[test]
    fn rejects_relation_without_evidence_in_strict_shape() {
        let mut request = sample_write_request();
        request["connect_to"][0]
            .as_object_mut()
            .expect("sample relation should be an object")
            .remove("evidence");

        let error = build_write_plan(&request).expect_err("relation evidence is required");

        assert_eq!(error, "missing required argument `connect_to[0].evidence`");
    }

    #[test]
    fn rejects_strict_write_without_any_relation_after_the_about_exists() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("connect_to");

        let error = build_write_plan(&request).expect_err("strict write requires a relation");

        assert_eq!(
            error,
            "strict kernel_write_memory requires at least one connect_to relation once the about exists; inspect or traverse a target first, or set options.strict=false when an unlinked write is intentional"
        );
    }

    #[test]
    fn accepts_the_first_strict_write_as_an_unlinked_about_root() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("connect_to");

        let plan = build_write_plan_with_root(&request, true)
            .expect("a server-proven new about may form its root");
        assert!(
            !plan
                .relations
                .iter()
                .any(|relation| relation == "chosen_because"),
            "the plan succeeds without inventing a link from the root"
        );
    }

    #[test]
    fn rejects_rich_relation_without_read_context_in_strict_mode() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("read_context");

        let error = build_write_plan(&request).expect_err("rich relation requires prior read");

        assert_eq!(
            error,
            "strict kernel_write_memory rich relation `chosen_because` to `incident:mobile-login:observation:401-refresh-race` requires read_context evidence; inspect, trace, or traverse the target first, or use an explicit anemic fallback"
        );
    }

    #[test]
    fn rejects_self_loop_relations() {
        let mut request = sample_write_request();
        let current_ref = "incident:mobile-login:entry:decision:self";
        request["current"]["ref"] = json!(current_ref);
        request["connect_to"][0]["ref"] = json!(current_ref);

        let error = build_write_plan(&request).expect_err("self-loop should fail");

        assert_eq!(
            error,
            "kernel_write_memory relation `chosen_because` cannot point from and to the same ref `incident:mobile-login:entry:decision:self`"
        );
    }

    #[test]
    fn classifies_explicit_anemic_fallback_relations() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("semantic_delta");
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("read_context");
        request["connect_to"][0]["rel"] = json!("follows");
        request["connect_to"][0]["class"] = json!("procedural");
        request["connect_to"][0]["why"] =
            json!("The new turn follows this prior process turn in sequence.");
        request["connect_to"][0]["evidence"] =
            json!("The writer only knows process succession for this memory.");

        let plan = build_write_plan(&request).expect("anemic fallback should be explicit");

        assert_eq!(plan.relation_quality.len(), 1);
        assert_eq!(plan.relation_quality[0]["quality"], "anemic");
        assert_eq!(plan.relation_quality[0]["fallback"], true);
        assert_eq!(
            plan.relation_quality_metrics["relation_anemic_count"],
            json!(1)
        );
    }

    #[test]
    fn accepts_core_operand_modeling_relations() {
        let mut request = sample_write_request();
        request["connect_to"][0]["rel"] = json!("matches_question_item");
        request["connect_to"][0]["class"] = json!("constraint");
        request["connect_to"][0]["why"] =
            json!("The prior memory satisfies the current requirement predicate.");
        request["connect_to"][0]["evidence"] =
            json!("The writer observed the target ref before linking it.");

        let plan = build_write_plan(&request).expect("operand relation should be accepted");

        assert_eq!(plan.relation_quality[0]["quality"], "rich");
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][0]["rel"],
            "matches_requirement"
        );
    }

    #[test]
    fn rejects_unsupported_relations_in_strict_mode() {
        let mut request = sample_write_request();
        request["connect_to"][0]["rel"] = json!("related_to");

        let error = build_write_plan(&request).expect_err("vague relation should fail");

        assert_eq!(
            error,
            "unsupported or vague kernel_write_memory relation `related_to`"
        );
    }

    #[test]
    fn rejects_duplicate_generated_refs() {
        let mut request = sample_write_request();
        let current_ref = "incident:mobile-login:entry:decision:duplicate";
        request["current"]["ref"] = json!(current_ref);
        request["semantic_delta"]["ref"] = json!(current_ref);

        let error = build_write_plan(&request).expect_err("duplicate refs should fail");

        assert_eq!(
            error,
            "generated duplicate memory ref `incident:mobile-login:entry:decision:duplicate`"
        );
    }

    fn sample_write_request() -> Value {
        json!({
            "about": "incident:mobile-login",
            "intent": "record_decision",
            "actor": "agent:backend",
            "observed_at": "2026-05-06T10:00:00Z",
            "scope": {
                "task": "incident:mobile-login",
                "process": "incident:mobile-login:resolution",
                "episode": "incident:mobile-login:episode:backend"
            },
            "current": {
                "kind": "decision",
                "summary": "Use token refresh retry instead of widening timeout.",
                "evidence": "Logs show 401 immediately after token refresh."
            },
            "semantic_delta": {
                "from": "The team suspected network timeout.",
                "to": "The evidence points to token refresh race.",
                "why": "The failing requests return 401 immediately after refresh.",
                "evidence": "Auth logs show refresh success followed by 401 on the next request."
            },
            "connect_to": [
                {
                    "ref": "incident:mobile-login:observation:401-refresh-race",
                    "rel": "chosen_because",
                    "class": "causal",
                    "why": "The decision addresses the observed token refresh race.",
                    "evidence": "The chosen retry targets the refresh race seen in auth logs."
                }
            ],
            "read_context": {
                "inspected_refs": [
                    "incident:mobile-login:observation:401-refresh-race"
                ]
            },
            "options": {
                "dry_run": true,
                "strict": true
            }
        })
    }
}
