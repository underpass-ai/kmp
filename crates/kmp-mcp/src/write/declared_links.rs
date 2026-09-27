//! The `connect_to` links of one record, compiled one at a time.
//!
//! One concept: what a single declared link needs to be written — a known
//! relation, a class it admits, its why and evidence, a confidence and the
//! prior context the strict writer asks for. Each link is judged on its own
//! so a record with two bad links hears about both.

use serde_json::{Value, json};
use std::collections::BTreeSet;

use kmp_domain::MemoryRelationType;

use super::compiled_link::CompiledLink;
use super::read_context::ReadContext;
use super::relation_quality::{RelationQualityInput, relation_quality_diagnostic};
use super::relations::{relation, resolve_class};
use super::validated_arguments::{
    optional_map_string, required_map_string, required_relation_string, validate_confidence,
    validate_semantic_class,
};
use super::validation_error::WriteValidationError;

const DEFAULT_CONFIDENCE: &str = "high";

/// The record every link of a `connect_to` starts from, and what judging a
/// link needs to know about it.
pub(super) struct DeclaredLinks<'a> {
    pub(super) about: &'a str,
    pub(super) from: &'a str,
    pub(super) actor: &'a str,
    pub(super) observed_at: Option<&'a str>,
    pub(super) strict: bool,
    pub(super) sequence: u32,
    pub(super) read_context: &'a ReadContext,
    pub(super) local_refs: &'a BTreeSet<String>,
}

impl DeclaredLinks<'_> {
    /// Compiles `connect_to[index]`, or says the first thing wrong with it.
    pub(super) fn compile(
        &self,
        index: usize,
        link: &Value,
    ) -> Result<CompiledLink, WriteValidationError> {
        let at = format!("connect_to[{index}]");
        let link = link.as_object().ok_or_else(|| {
            WriteValidationError::new(format!("{at} must be an object")).at(at.clone())
        })?;
        let target_ref = required_map_string(link, "ref", &format!("{at}.ref"))?;
        let rel_arg = required_map_string(link, "rel", &format!("{at}.rel"))?;
        let relation_type = MemoryRelationType::new(rel_arg).map_err(|error| {
            WriteValidationError::new(format!("{at}.rel is invalid: {error}"))
                .at(format!("{at}.rel"))
        })?;
        let rel = relation_type.as_str();
        let semantic_class =
            resolve_class(link, rel, self.strict).map_err(|error| error.within(&at))?;
        validate_semantic_class(semantic_class)
            .map_err(|error| WriteValidationError::new(error).at(format!("{at}.class")))?;
        let why = required_relation_string(link, "why", semantic_class, index)?;
        let evidence = required_relation_string(link, "evidence", semantic_class, index)?;
        let confidence = optional_map_string(link, "confidence").unwrap_or(DEFAULT_CONFIDENCE);
        validate_confidence(confidence)
            .map_err(|error| WriteValidationError::new(error).at(format!("{at}.confidence")))?;
        let quality = relation_quality_diagnostic(RelationQualityInput {
            about: self.about,
            from: self.from,
            to: target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            evidence,
            strict: self.strict,
            read_context: self.read_context,
            local_refs: self.local_refs,
        })
        .map_err(|error| error.within(&at))?;

        let mut compiled = relation(
            self.from,
            target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            evidence,
            self.sequence,
        );
        // An equivalence across abouts carries the proposal it was declared
        // from as its method, which is what the kernel admits it by.
        let crosses_about = quality["crosses_about"] == true;
        if crosses_about {
            let proposal = self
                .read_context
                .relate_proposal_for(self.about, target_ref)
                .ok_or_else(|| {
                    WriteValidationError::new("cross-about equivalence without its proposal")
                        .at(at.clone())
                })?;
            compiled["method"] = json!(format!(
                "{}:{}",
                kmp_domain::DECLARED_FROM_RELATE_METHOD,
                proposal.proposed_by.join("+")
            ));
        }
        // A structural link is exempt from evidence, and an evidence item with
        // no text is not evidence: the canonical ingest mapper requires
        // `memory.evidence[].text`, and rightly refuses an empty one. The
        // evidence node supports what this about owns; a ref of another
        // about is named by the relation, never claimed by the evidence.
        let evidence = (!evidence.trim().is_empty()).then(|| {
            let supports = if crosses_about {
                json!([self.from])
            } else {
                json!([self.from, target_ref])
            };
            json!({
                "id": format!("evidence:{}:relation:{}", self.from, index + 1),
                "supports": supports,
                "text": evidence,
                "source": format!("kmp_write_memory:{}:relation:{rel}", self.actor),
                "time": self.observed_at
            })
        });
        Ok(CompiledLink {
            relation: compiled,
            name: rel.to_string(),
            quality,
            evidence,
        })
    }
}
