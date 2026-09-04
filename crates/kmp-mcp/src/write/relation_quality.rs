//! Whether a relation carries its why, and how honestly to say so.
//!
//! One concept: relation quality. A rich relation names a specific type, a
//! semantic class, a why, concrete evidence and a confidence; the anemic types
//! are an honest fallback for when no richer dependency can be proven, never a
//! default. This module decides which a writer produced and reports it — it
//! never invents a rationale to make a relation look richer than it is.
//!
//! It is policy, not parsing: it takes what the caller already supplied and
//! judges it. Extracting arguments from JSON stays with the writer.

use kmp_domain::{MemoryRelationQuality, MemoryRelationType, RelationSemanticClass};
use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::read_context::ReadContext;
use super::relations::{NON_STRUCTURAL_RELATION_CLASSES, ResolvedRelationSpec};

#[derive(Clone, Copy, Debug)]
pub(super) struct RelationQualityInput<'a> {
    /// The about the write belongs to: what decides whether a target is
    /// this about's, or another's.
    pub(super) about: &'a str,
    pub(super) from: &'a str,
    pub(super) to: &'a str,
    pub(super) rel: &'a str,
    pub(super) semantic_class: &'a str,
    pub(super) confidence: &'a str,
    pub(super) why: &'a str,
    pub(super) evidence: &'a str,
    pub(super) strict: bool,
    pub(super) read_context: &'a ReadContext,
    pub(super) local_refs: &'a BTreeSet<String>,
}

pub(super) fn relation_quality_diagnostic(
    input: RelationQualityInput<'_>,
) -> Result<Value, String> {
    let spec = relation_spec(input.rel, input.strict)?;
    let semantic_class = RelationSemanticClass::parse(input.semantic_class)
        .map_err(|error| format!("kmp_write_memory relation class is invalid: {error}"))?;
    if !spec.classes.contains(&semantic_class) {
        return Err(format!(
            "kmp_write_memory relation `{}` cannot use class `{}`; expected one of {}",
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
            "kmp_write_memory relation `{}` requires a target ref",
            input.rel
        ));
    }
    if input.from == input.to {
        return Err(format!(
            "kmp_write_memory relation `{}` cannot point from and to the same ref `{}`",
            input.rel, input.from
        ));
    }
    if input.strict && input.semantic_class != "structural" && !proof_complete {
        return Err(format!(
            "kmp_write_memory relation `{}` requires both why and evidence in strict mode",
            input.rel
        ));
    }

    // The one relation that may cross an about: an equivalence, with why and
    // evidence, declared from a proposal `kmp_relate` returned. Anything
    // else pointing outside the about is refused here, before the kernel
    // sees it, with the way to do it right.
    let target_is_local = input.local_refs.contains(input.to);
    let target_is_foreign = !target_is_local
        && kmp_application::validate_supplied_member_ref(input.about, "connect_to[].ref", input.to)
            .is_err();
    if target_is_foreign {
        let may_cross = MemoryRelationType::new(input.rel)
            .is_ok_and(|relation_type| relation_type.may_cross_abouts());
        if !may_cross {
            return Err(format!(
                "kmp_write_memory can connect to `{}` of another about only with `same_event_as` or `same_entity_as`, declared from a kmp_relate proposal; `{}` stays inside about `{}`",
                input.to, input.rel, input.about
            ));
        }
        if input
            .read_context
            .relate_proposal_for(input.about, input.to)
            .is_none()
        {
            return Err(format!(
                "kmp_write_memory equivalence `{}` to `{}` of another about requires the kmp_relate proposal in read_context.relate_proposals: its from, to and proposed_by as kmp_relate returned them, one of the two refs belonging to about `{}`",
                input.rel, input.to, input.about
            ));
        }
    }
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
            "strict kmp_write_memory rich relation `{}` to `{}` requires read_context evidence; inspect, trace, or traverse the target first, or use an explicit anemic fallback",
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
        "crosses_about": target_is_foreign,
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

pub(crate) fn relation_spec(rel: &str, strict: bool) -> Result<ResolvedRelationSpec, String> {
    let relation_type = MemoryRelationType::new(rel)
        .map_err(|error| format!("kmp_write_memory relation type is invalid: {error}"))?;
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
        "unsupported or vague kmp_write_memory relation `{rel}`"
    ))
}

pub(crate) fn relation_quality_reason(
    quality: MemoryRelationQuality,
    default_reason: &str,
) -> &str {
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

pub(crate) fn class_names(classes: &[RelationSemanticClass]) -> Vec<&'static str> {
    classes.iter().map(RelationSemanticClass::as_str).collect()
}

pub(super) fn relation_quality_metrics(relation_quality: &[Value]) -> Value {
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

pub(crate) fn quality_count(relation_quality: &[Value], quality: &str) -> usize {
    relation_quality
        .iter()
        .filter(|relation| relation["quality"].as_str() == Some(quality))
        .count()
}

pub(crate) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}
