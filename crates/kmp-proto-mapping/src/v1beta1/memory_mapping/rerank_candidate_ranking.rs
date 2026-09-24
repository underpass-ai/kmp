use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;
use sha2::{Digest, Sha256};

use super::answer_selection::{mark_reached_by, stable_evidence_key};
use super::scalars::{ProtoMappingResult, invalid_argument};

/// Admitted entries reordered by a remote judgement of whether each answers
/// the question. Like the semantic channel it names identities and content
/// fingerprints, never text; the mapping resolves them against admitted
/// stored evidence, and anything reached only this way stays indirect.
#[derive(Debug, Clone)]
pub struct RerankCandidateRanking {
    model: String,
    question_digest: [u8; 32],
    fingerprints: Vec<(String, String)>,
}

impl RerankCandidateRanking {
    /// At most 100 unique `(entry ref, SHA-256 of exact text)` identities,
    /// best first, from a pinned model. One entry may appear once per text
    /// that supports it: its body and each evidence text are judged apart.
    pub fn new(
        model: String,
        question: &str,
        fingerprints: Vec<(String, String)>,
    ) -> ProtoMappingResult<Self> {
        let mut identities = BTreeSet::new();
        let valid = !model.trim().is_empty()
            && model.len() <= 256
            && fingerprints.len() <= 100
            && fingerprints.iter().all(|(entry_ref, fingerprint)| {
                !entry_ref.is_empty()
                    && entry_ref.len() <= 4096
                    && fingerprint.len() == 64
                    && fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
                    && identities.insert((entry_ref, fingerprint.to_ascii_lowercase()))
            });
        if !valid {
            return Err(invalid_argument("invalid rerank candidate ranking"));
        }
        Ok(Self {
            model,
            question_digest: Sha256::digest(question.as_bytes()).into(),
            fingerprints: fingerprints
                .into_iter()
                .map(|(entry_ref, hash)| (entry_ref, hash.to_ascii_lowercase()))
                .collect(),
        })
    }

    /// The ranking resolved against an already admitted, live pool.
    pub(super) fn resolve(&self, admitted: &[MemoryEvidence]) -> Vec<MemoryEvidence> {
        let mut by_identity = BTreeMap::new();
        for item in admitted {
            let fingerprint = format!("{:x}", Sha256::digest(item.text.as_bytes()));
            for entry_ref in &item.supports {
                by_identity
                    .entry((entry_ref.clone(), fingerprint.clone()))
                    .and_modify(|current: &mut &MemoryEvidence| {
                        if stable_evidence_key(item) < stable_evidence_key(current) {
                            *current = item;
                        }
                    })
                    .or_insert(item);
            }
        }
        self.fingerprints
            .iter()
            .filter_map(|key| by_identity.get(key))
            .map(|item| {
                let mut item = mark_reached_by((*item).clone(), "rerank");
                item.metadata
                    .insert("rerank_model".to_string(), self.model.clone());
                item
            })
            .collect()
    }

    pub(super) fn matches_question(&self, question: &str) -> bool {
        self.question_digest == <[u8; 32]>::from(Sha256::digest(question.as_bytes()))
    }

    pub(super) fn len(&self) -> usize {
        self.fingerprints.len()
    }

    pub(super) fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(id: &str, supports: &str, text: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.into(),
            supports: vec![supports.into()],
            text: text.into(),
            ..MemoryEvidence::default()
        }
    }

    fn sha(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    #[test]
    fn resolves_only_admitted_identities_in_ranked_order_and_marks_them() {
        let admitted = vec![evidence("e1", "a:1", "one"), evidence("e2", "a:2", "two")];
        let ranking = RerankCandidateRanking::new(
            "jev-1.13.0".into(),
            "q",
            vec![
                ("a:2".into(), sha("two")),
                ("a:9".into(), sha("nine")),
                ("a:1".into(), sha("changed")),
            ],
        )
        .expect("ranking");
        let resolved = ranking.resolve(&admitted);
        assert_eq!(resolved.len(), 1, "unknown and changed text never resolve");
        assert_eq!(resolved[0].id, "e2");
        assert_eq!(resolved[0].metadata["reached_by"], "rerank");
        assert_eq!(resolved[0].metadata["rerank_model"], "jev-1.13.0");
        assert!(ranking.matches_question("q") && !ranking.matches_question("other"));
    }

    #[test]
    fn one_entry_may_be_judged_through_each_of_its_texts() {
        // An entry is supported by its own body and by evidence text; the
        // pool offers each text once, so identities are pairs, not refs.
        let admitted = vec![
            evidence("e1", "a:1", "body"),
            evidence("e2", "a:1", "evidence"),
        ];
        let ranking = RerankCandidateRanking::new(
            "m".into(),
            "q",
            vec![("a:1".into(), sha("evidence")), ("a:1".into(), sha("body"))],
        )
        .expect("two texts of one entry");
        let resolved = ranking.resolve(&admitted);
        assert_eq!(
            resolved
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["e2", "e1"]
        );
    }

    #[test]
    fn duplicate_identities_and_bad_fingerprints_are_refused() {
        for fingerprints in [
            vec![("a".to_string(), sha("x")), ("a".to_string(), sha("x"))],
            vec![("a".to_string(), "short".to_string())],
        ] {
            assert!(RerankCandidateRanking::new("m".into(), "q", fingerprints).is_err());
        }
    }
}
