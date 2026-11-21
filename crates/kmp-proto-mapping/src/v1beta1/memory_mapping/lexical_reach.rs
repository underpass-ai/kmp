use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::ref_words;
use super::answer_recall_context::relation_is_explanatory;
use super::answer_selection::answer_context_refs;
use super::lexical_profile::LexicalProfile;
use super::lexical_row::LexicalRow;
use super::reach_relation::ReachRelation;
use super::search_terms::informative_terms;
use super::varint::{Reader, push_bytes, push_unsigned};

/// The layout of an encoded reach; a different one is refused.
const REACH_LAYOUT: u64 = 1;

/// The words a candidate's graph can lend it beyond its own row (DESIGN L6,
/// P14): every word the claims it supports and the relations around them can
/// carry, less the words its direct field already holds.
///
/// A strict policy counts a candidate's focus over its direct field, the
/// words of the refs it supports and their details (`claim`) and the
/// explanatory relations incident to its refs (`relation`). The ranker reads
/// those from the part of the about an ask read, which is never more than
/// the about's selection keeps; this reads them from everything the
/// selection keeps around the candidate. So the focus it can reach in any
/// reading is at most the focus counted over its direct field and these
/// words, and a candidate whose bound stays below the focus required can be
/// eligible in no reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LexicalReach {
    /// Ascending, each once, none of them in the row's direct field.
    terms: Vec<String>,
}

impl LexicalReach {
    /// The refs whose details and incident relations a candidate's focus
    /// may count: the node a detail candidate stands for and every ref it
    /// supports.
    pub fn refs(item: &MemoryEvidence) -> BTreeSet<String> {
        answer_context_refs(item)
    }

    /// The reach of `item` (whose row is `row`) in `profile`'s language,
    /// from the details of its refs and the kept relations incident to them
    /// (any relation that is not explanatory is skipped here, as the ranker
    /// skips it).
    pub fn read<'a>(
        item: &MemoryEvidence,
        row: &LexicalRow,
        profile: &LexicalProfile,
        details: impl IntoIterator<Item = &'a str>,
        relations: impl IntoIterator<Item = ReachRelation<'a>>,
    ) -> Self {
        let morphology = profile.morphology();
        let mut terms = BTreeSet::new();
        for supported in &item.supports {
            terms.extend(informative_terms(&ref_words(supported), &morphology));
        }
        for detail in details {
            terms.extend(informative_terms(detail, &morphology));
        }
        for relation in relations {
            if !relation_is_explanatory(relation.relation_type, relation.explanation) {
                continue;
            }
            let explanation = relation.explanation;
            terms.extend(informative_terms(
                &format!("{} {}", relation.source, relation.target),
                &morphology,
            ));
            terms.extend(informative_terms(
                explanation.evidence().unwrap_or_default(),
                &morphology,
            ));
            // The ranker counts the rationale only beside evidence; a reach
            // may hold more than it counts.
            terms.extend(informative_terms(
                &format!(
                    "{} {}",
                    explanation.rationale().unwrap_or_default(),
                    explanation.motivation().unwrap_or_default()
                ),
                &morphology,
            ));
            terms.extend(informative_terms(relation.relation_type, &morphology));
        }
        let direct = row.direct_counts(false);
        Self {
            terms: terms
                .into_iter()
                .filter(|term| direct.count(term) == 0)
                .collect(),
        }
    }

    /// The words, ascending.
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        push_unsigned(&mut buffer, REACH_LAYOUT);
        push_unsigned(&mut buffer, self.terms.len() as u64);
        for term in &self.terms {
            push_bytes(&mut buffer, term.as_bytes());
        }
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let layout = reader.unsigned()?;
        if layout != REACH_LAYOUT {
            return Err(format!(
                "lexical reach layout {layout} is not {REACH_LAYOUT}"
            ));
        }
        let count = reader.unsigned()?;
        let mut terms = Vec::with_capacity(count.min(4096) as usize);
        for _ in 0..count {
            let term = std::str::from_utf8(reader.bytes()?)
                .map_err(|error| format!("lexical reach term: {error}"))?;
            terms.push(term.to_string());
        }
        if !reader.finished() {
            return Err("lexical reach has trailing bytes".into());
        }
        Ok(Self { terms })
    }
}

#[cfg(test)]
#[path = "lexical_reach_tests.rs"]
mod tests;
