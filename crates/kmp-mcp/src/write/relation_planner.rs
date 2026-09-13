//! The write that links two memories that already exist: `kmp_write_memory`
//! with `relations`.
//!
//! Adding an evidenced link used to cost a source rewrite. The only shapes
//! the writer accepted were `memories` and `search_summaries`, so a writer
//! that wanted to say "J01 supports J03" had to resubmit J01 as a record —
//! and a record carries its text, its evidence, its coordinates, its labels
//! and its metadata with it. The link arrived; J01's prose left with it.
//!
//! This path takes the link and nothing else. Both endpoints are read from
//! the store and written back byte for byte: same ref, same kind, same text,
//! same coordinates, same metadata. What the caller supplies is the relation
//! — its direction, its class, its why, its evidence and the observation at
//! which it is asserted — and that assertion carries its own clock, so an
//! old source is not backdated into a link declared today.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use kmp_application::{validate_ref_token, validate_supplied_entry_ref};
use kmp_domain::MemoryRelationType;

use super::existing_entry::ExistingEntry;
use super::generated_ref::{short_hash, stable_idempotency_key};
use super::json_value_type::JsonValueType;
use super::plan::KernelWritePlan;
use super::proof_observation::proof_clocks;
use super::relation_quality::{
    RelationQualityInput, relation_quality_diagnostic, relation_quality_metrics,
};
use super::relations::{relation, resolve_class};
use super::review_token::validate_review_token;
use super::validated_arguments::{
    optional_map_string, optional_string, required_map_string, required_string,
    validate_confidence, validate_semantic_class,
};
use super::validation_error::WriteValidationError;
use super::validation_errors::WriteValidationErrors;

const DEFAULT_CONFIDENCE: &str = "high";
const DEFAULT_SOURCE_KIND: &str = "agent";

/// Coordinates, labels and validity belong to the memories this write does
/// not touch. A relation's own occurrence and validity stay unknown.
const PRESERVED_FIELDS: [&str; 5] = ["labels", "occurred_at", "valid_from", "valid_until", "rank"];

/// Fields that declare one of the other two shapes.
const FOREIGN_FIELDS: [&str; 7] = [
    "memories",
    "search_summaries",
    "current",
    "intent",
    "semantic_delta",
    "connect_to",
    "scope",
];

/// The about and the stored memories a relation-only packet must be read
/// against before it can be planned: every endpoint this about owns.
///
/// This is a reading list, not the validation. The planner remains the
/// authority on every field; it simply cannot read storage itself.
pub(crate) fn declared_sources(
    arguments: &Value,
) -> Result<(String, Vec<String>), WriteValidationErrors> {
    let object = arguments
        .as_object()
        .ok_or("tool arguments must be an object")?;
    let about = required_string(object, "about")?;
    validate_ref_token("about", &about)?;
    let mut refs = Vec::new();
    for (index, link) in declared_relations(object)?.iter().enumerate() {
        let link = relation_object(link, index)?;
        let from = required_map_string(link, "from", &format!("relations[{index}].from"))?;
        validate_supplied_entry_ref(&about, &format!("relations[{index}].from"), from).map_err(
            |error| {
                WriteValidationError::new(error)
                    .at(format!("relations[{index}].from"))
                    .code("INVALID_REF")
            },
        )?;
        let to = required_map_string(link, "to", &format!("relations[{index}].to"))?;
        for endpoint in [from, to] {
            // A cross-about equivalence names a ref this about does not own.
            // It is admitted by the kernel against the other about and is
            // never rewritten from here, so it is not read back.
            if validate_supplied_entry_ref(&about, "relations[].to", endpoint).is_ok()
                && !refs.iter().any(|known| known == endpoint)
            {
                refs.push(endpoint.to_owned());
            }
        }
    }
    Ok((about, refs))
}

/// Compiles the packet against the stored endpoints `declared_sources` named.
pub(crate) fn build_relation_plan(
    arguments: &Value,
    sources: &BTreeMap<String, ExistingEntry>,
) -> Result<KernelWritePlan, WriteValidationErrors> {
    let object = arguments
        .as_object()
        .ok_or("tool arguments must be an object")?;
    let about = required_string(object, "about")?;
    validate_ref_token("about", &about)?;
    let actor = required_string(object, "actor")?;
    let packet_observation = super::coordinates::observation_time(object)?;
    reject_fields_that_move_a_source(object)?;
    validate_review_token(object.get("review_token"))?;
    let read_context = super::read_context::ReadContext::from_arguments(object)
        .map_err(|error| WriteValidationError::new(error).at("read_context").global())?;
    let options = object.get("options").and_then(Value::as_object);
    let dry_run = options
        .and_then(|options| options.get("dry_run"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let strict = options
        .and_then(|options| options.get("strict"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let identity = optional_string(object.get("idempotency_key"))
        .map(str::to_owned)
        .unwrap_or_else(|| stable_idempotency_key(object));

    let declared = declared_relations(object)?;
    // Every endpoint is stored, so nothing in this packet vouches for a
    // target: prior context is what `read_context` declares and what the
    // served neighborhood shows, never "the current request".
    let no_local_refs = BTreeSet::new();
    let mut preserved: Vec<&ExistingEntry> = Vec::new();
    let mut relations = Vec::new();
    let mut relation_names = Vec::new();
    let mut relation_quality = Vec::new();
    let mut evidence = Vec::new();
    let mut diagnostics = Vec::new();
    let mut declared_links = BTreeSet::new();
    let mut errors = Vec::new();
    for (index, link) in declared.iter().enumerate() {
        let compiled = (|| {
            let link = relation_object(link, index)?;
            let from = required_map_string(link, "from", "from")?;
            let to = required_map_string(link, "to", "to")?;
            let rel = MemoryRelationType::new(required_map_string(link, "rel", "rel")?)
                .map_err(|error| {
                    WriteValidationError::new(format!("relations[].rel is invalid: {error}"))
                        .at("rel")
                        .code("INVALID_RELATION")
                })?;
            let rel = rel.as_str();
            let semantic_class = resolve_class(link, rel, strict)?;
            validate_semantic_class(semantic_class)
                .map_err(|error| WriteValidationError::new(error).at("class"))?;
            if semantic_class == "structural" {
                return Err(WriteValidationError::new(format!(
                    "`relations` declares evidenced links; the structural link `{rel}` carries a coordinate rather than a rationale, so change memberships with kmp_relabel instead"
                ))
                .at("class")
                .code("STRUCTURAL_RELATION_UNSUPPORTED"));
            }
            if !declared_links.insert((from.to_owned(), rel.to_owned(), to.to_owned())) {
                return Err(WriteValidationError::new(format!(
                    "relations repeats `{from}` -> {rel} -> `{to}`; declare each link once"
                ))
                .code("DUPLICATE_RELATION"));
            }
            let why = required_map_string(link, "why", "why").map_err(|error| {
                error.code("RELATION_PROOF_REQUIRED")
            })?;
            let proof = required_map_string(link, "evidence", "evidence").map_err(|error| {
                error.code("RELATION_PROOF_REQUIRED")
            })?;
            let confidence = optional_map_string(link, "confidence").unwrap_or(DEFAULT_CONFIDENCE);
            validate_confidence(confidence)
                .map_err(|error| WriteValidationError::new(error).at("confidence"))?;
            let observed = relation_observation(link, packet_observation.as_deref())?;
            let quality = relation_quality_diagnostic(RelationQualityInput {
                about: &about,
                from,
                to,
                rel,
                semantic_class,
                confidence,
                why,
                evidence: proof,
                strict,
                read_context: &read_context,
                local_refs: &no_local_refs,
            })?;
            let mut compiled = relation(
                from,
                to,
                rel,
                semantic_class,
                confidence,
                why,
                proof,
                u32::try_from(index + 1).unwrap_or(u32::MAX),
            );
            // The assertion is dated by the writer, never by the memories it
            // joins. An empty clock object asks the kernel for its ingestion
            // instant rather than borrowing an endpoint's observation.
            compiled["clocks"] =
                proof_clocks(observed.as_deref().map(|value| json!(value)).as_ref());
            let crosses_about = quality["crosses_about"] == true;
            if crosses_about {
                let proposal = read_context
                    .relate_proposal_for(&about, to)
                    .ok_or_else(|| "cross-about equivalence without its proposal".to_string())?;
                compiled["method"] = json!(format!(
                    "{}:{}",
                    kmp_domain::DECLARED_FROM_RELATE_METHOD,
                    proposal.proposed_by.join("+")
                ));
            }
            Ok(CompiledRelation {
                from: from.to_owned(),
                to: to.to_owned(),
                rel: rel.to_owned(),
                crosses_about,
                observed,
                proof: proof.to_owned(),
                quality,
                value: compiled,
            })
        })()
        .map_err(|error: WriteValidationError| error.within(&format!("relations[{index}]")));
        match compiled {
            Ok(compiled) => {
                for endpoint in [&compiled.from, &compiled.to] {
                    if let Some(source) = sources.get(endpoint.as_str())
                        && !preserved
                            .iter()
                            .any(|kept| kept.reference == source.reference)
                    {
                        preserved.push(source);
                    }
                }
                let clocks = compiled.value["clocks"].clone();
                evidence.push(json!({
                    "id": relation_evidence_ref(&compiled, &identity, index),
                    "supports": if compiled.crosses_about {
                        json!([compiled.from])
                    } else {
                        json!([compiled.from, compiled.to])
                    },
                    "text": compiled.proof,
                    "source": format!("kmp_write_memory:{actor}:relation:{}", compiled.rel),
                    "time": compiled.observed,
                    "support_clocks": clocks
                }));
                diagnostics.push(format!(
                    "attached `{}` from `{}` to `{}`; both sources keep their stored text, coordinates, labels, metadata and earlier evidence",
                    compiled.rel, compiled.from, compiled.to
                ));
                relation_names.push(compiled.rel.clone());
                relation_quality.push(compiled.quality.clone());
                relations.push(compiled.value);
            }
            Err(error) => errors.push(error),
        }
    }
    if let Some(errors) = WriteValidationErrors::collected(errors) {
        return Err(errors);
    }

    let entries = preserved
        .iter()
        .map(|source| {
            json!({
                "id": source.reference,
                "kind": source.kind,
                "text": source.text,
                "coordinates": source.coordinates,
                "metadata": source.metadata
            })
        })
        .collect::<Vec<_>>();
    let mut ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": identity.clone(),
        "dry_run": dry_run,
        "default_observation_to_ingestion": true,
        "label_policy": if strict { "refuse" } else { "warn" },
        "memory": {
            // The about already holds every dimension these coordinates
            // stand in; declaring one here would be a membership change.
            "dimensions": [],
            "entries": entries,
            "relations": relations,
            "evidence": evidence
        },
        "provenance": {
            "source_kind": object
                .get("source_kind")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SOURCE_KIND),
            "source_agent": actor,
            "observed_at": packet_observation,
            "correlation_id": format!("kmp_write:{about}"),
            "causation_id": identity
        }
    });
    super::coordinates::omit_unknown_observations(&mut ingest_arguments);
    let next_suggested_reads = ingest_arguments["memory"]["relations"][0]
        .as_object()
        .map(|first| {
            vec![json!({
                "tool": "kmp_trace",
                "from": first["from"],
                "to": first["to"]
            })]
        })
        .unwrap_or_default();
    let relation_quality_metrics = relation_quality_metrics(&relation_quality);
    Ok(KernelWritePlan {
        operation: super::operation::WriteOperation::Relations,
        about,
        local_refs: Default::default(),
        dry_run,
        ingest_arguments,
        generated_refs: Vec::new(),
        labels: Vec::new(),
        relations: relation_names,
        relation_quality,
        relation_quality_metrics,
        replaced: Vec::new(),
        diagnostics,
        next_suggested_reads,
    })
}

/// One validated link and everything the packet needs to remember about it.
struct CompiledRelation {
    from: String,
    to: String,
    rel: String,
    crosses_about: bool,
    observed: Option<String>,
    proof: String,
    quality: Value,
    value: Value,
}

/// The evidence a relation generates is identified by the assertion, never
/// by its ordinal in one request.
///
/// `evidence:<from>:relation:1` was the second relation ever declared from
/// `from` losing to the first: two independent writes, one id, and the
/// earlier evidence text replaced by the later one. Deriving the suffix from
/// the logical write identity and the triple keeps an exact retry on the
/// same id — which is what makes the retry a replay — while a genuinely new
/// link gets a new evidence node beside the old one.
fn relation_evidence_ref(compiled: &CompiledRelation, identity: &str, index: usize) -> String {
    let suffix = short_hash(&format!(
        "{identity}\0{}\0{}\0{}\0{index}",
        compiled.from, compiled.rel, compiled.to
    ));
    format!("evidence:{}:relation:{suffix}", compiled.from)
}

/// When the link was asserted: the relation's own value, the packet's, or —
/// for an explicit null — the exact ingestion instant.
fn relation_observation(
    link: &Map<String, Value>,
    packet: Option<&str>,
) -> Result<Option<String>, WriteValidationError> {
    let Some(value) = link.get("observed_at") else {
        return Ok(packet.map(str::to_owned));
    };
    if value.is_null() {
        return Ok(None);
    }
    let observed = value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            WriteValidationError::new(
                "observed_at must be an RFC3339 timestamp or null; omit it for the packet observation",
            )
            .at("observed_at")
            .code("INVALID_OBSERVATION")
        })?;
    super::coordinates::reject_a_time_that_has_not_happened(observed, crate::clock::now_seconds())
        .map_err(|error| {
            WriteValidationError::new(error)
                .at("observed_at")
                .code("FUTURE_OBSERVATION")
        })?;
    Ok(Some(observed.to_owned()))
}

fn declared_relations(object: &Map<String, Value>) -> Result<&Vec<Value>, WriteValidationError> {
    let relations = object.get("relations").ok_or_else(|| {
        WriteValidationError::new("relations is required")
            .at("relations")
            .code("REQUIRED_FIELD")
    })?;
    let declared = relations.as_array().ok_or_else(|| {
        WriteValidationError::wrong_type("relations", JsonValueType::Array, relations)
    })?;
    if declared.is_empty() {
        return Err(
            WriteValidationError::new("relations must contain at least one link")
                .at("relations")
                .code("EMPTY_RELATIONS"),
        );
    }
    Ok(declared)
}

fn relation_object(
    link: &Value,
    index: usize,
) -> Result<&Map<String, Value>, WriteValidationError> {
    link.as_object().ok_or_else(|| {
        WriteValidationError::wrong_type(
            &format!("relations[{index}]"),
            JsonValueType::Object,
            link,
        )
    })
}

fn reject_fields_that_move_a_source(
    object: &Map<String, Value>,
) -> Result<(), WriteValidationError> {
    for field in FOREIGN_FIELDS {
        if object.contains_key(field) {
            return Err(WriteValidationError::new(format!(
                "`{field}` cannot accompany relations; a relation-only write declares links between memories that already exist"
            ))
            .at(field)
            .code("WRITE_OPERATION_REQUIRED"));
        }
    }
    for field in PRESERVED_FIELDS {
        if object.contains_key(field) {
            return Err(WriteValidationError::new(format!(
                "relations preserves each source's stored coordinates and labels, and a link's own occurrence and validity stay unknown; `{field}` cannot accompany it"
            ))
            .at(field)
            .code("PRESERVED_FIELD"));
        }
    }
    for field in ["sequence", "labels_new"] {
        if object
            .get("options")
            .is_some_and(|options| options.get(field).is_some())
        {
            return Err(WriteValidationError::new(format!(
                "options.{field} does not apply to relations"
            ))
            .at(format!("options.{field}"))
            .code("INAPPLICABLE_OPTION"));
        }
    }
    Ok(())
}
