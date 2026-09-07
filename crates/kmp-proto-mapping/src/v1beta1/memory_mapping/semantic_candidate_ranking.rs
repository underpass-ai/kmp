use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;
use sha2::{Digest, Sha256};

use super::answer_selection::{mark_reached_by, stable_evidence_key};
use super::scalars::{ProtoMappingResult, invalid_argument};

/// Ranked entry refs proposed by an optional semantic retriever.
///
/// This boundary accepts identities and content fingerprints, never generated
/// evidence. The caller must bind the ranking to the current question and
/// snapshot. The mapping resolves every proposal against admitted stored text.
/// Model/revision is provenance, not evidence of relevance or confidence.
#[derive(Debug, Clone)]
pub struct SemanticCandidateRanking {
    model_revision: String,
    question_digest: [u8; 32],
    fingerprints: Vec<(String, String)>,
    lexical_fingerprints: Option<Vec<(String, String)>>,
}

impl SemanticCandidateRanking {
    /// At most 100 ranked, unique entry refs with SHA-256 of their exact text.
    /// Use an immutable model revision so retrieval can be reproduced.
    pub fn new(
        model_revision: String,
        question: &str,
        fingerprints: Vec<(String, String)>,
    ) -> ProtoMappingResult<Self> {
        if model_revision.trim().is_empty()
            || model_revision.len() > 256
            || !valid_fingerprints(&fingerprints)
        {
            return Err(invalid_argument("invalid semantic candidate ranking"));
        }
        Ok(Self {
            model_revision,
            question_digest: Sha256::digest(question.as_bytes()).into(),
            fingerprints: fingerprints
                .into_iter()
                .map(|(entry_ref, hash)| (entry_ref, hash.to_ascii_lowercase()))
                .collect(),
            lexical_fingerprints: None,
        })
    }

    /// A second, unfused BM25 channel. Each channel may propose at most 100
    /// unique refs; neither its scores nor its text are accepted as evidence.
    pub fn with_lexical_candidates(
        mut self,
        fingerprints: Vec<(String, String)>,
    ) -> ProtoMappingResult<Self> {
        if !valid_fingerprints(&fingerprints) {
            return Err(invalid_argument("invalid lexical candidate ranking"));
        }
        self.lexical_fingerprints = Some(
            fingerprints
                .into_iter()
                .map(|(entry_ref, hash)| (entry_ref, hash.to_ascii_lowercase()))
                .collect(),
        );
        Ok(self)
    }

    /// Only the already scoped, temporally admitted and live pool is supplied.
    pub(super) fn resolve_channels(&self, admitted: &[MemoryEvidence]) -> Vec<Vec<MemoryEvidence>> {
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
        let mut channels = vec![(&self.fingerprints, "dense")];
        if let Some(lexical) = &self.lexical_fingerprints {
            channels.push((lexical, "bm25"));
        }
        channels
            .into_iter()
            .map(|(fingerprints, channel)| {
                fingerprints
                    .iter()
                    .filter_map(|key| by_identity.get(key))
                    .map(|item| {
                        let mut item = mark_reached_by((*item).clone(), "semantic");
                        item.metadata.insert(
                            "semantic_model_revision".to_string(),
                            self.model_revision.clone(),
                        );
                        if self.lexical_fingerprints.is_some() {
                            item.metadata
                                .insert("retrieval_channel".into(), channel.into());
                        }
                        item
                    })
                    .collect()
            })
            .collect()
    }

    pub(super) fn matches_question(&self, question: &str) -> bool {
        self.question_digest == <[u8; 32]>::from(Sha256::digest(question.as_bytes()))
    }

    pub(super) fn len(&self) -> usize {
        self.fingerprints
            .iter()
            .chain(self.lexical_fingerprints.iter().flatten())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

fn valid_fingerprints(fingerprints: &[(String, String)]) -> bool {
    let mut refs = BTreeSet::new();
    fingerprints.len() <= 100
        && fingerprints.iter().all(|(entry_ref, fingerprint)| {
            !entry_ref.is_empty()
                && entry_ref.len() <= 4096
                && refs.insert(entry_ref)
                && fingerprint.len() == 64
                && fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> MemoryEvidence {
        MemoryEvidence {
            id: "detail:entry:a".into(),
            supports: vec!["entry:a".into()],
            text: "The launch was postponed.".into(),
            source: "original".into(),
            ..Default::default()
        }
    }

    #[test]
    fn resolves_exact_stored_text_and_marks_similarity_as_retrieval_only() {
        let original = evidence();
        let ranking = SemanticCandidateRanking::new(
            "encoder@revision".into(),
            "query",
            vec![(
                "entry:a".into(),
                format!("{:x}", Sha256::digest(original.text.as_bytes())),
            )],
        )
        .expect("valid test fixture");
        let resolved = &ranking.resolve_channels(std::slice::from_ref(&original))[0];
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].text, original.text);
        assert_eq!(resolved[0].source, original.source);
        assert_eq!(resolved[0].metadata["reached_by"], "semantic");
        assert!(ranking.resolve_channels(&[])[0].is_empty());
        let mut changed = original;
        changed.text = "The launch was not postponed.".into();
        assert!(ranking.resolve_channels(&[changed])[0].is_empty());
    }

    #[test]
    fn rejects_unbounded_duplicate_and_malformed_rankings() {
        let pair = ("entry:a".into(), "a".repeat(64));
        assert!(
            SemanticCandidateRanking::new("m@r".into(), "query", vec![pair.clone(), pair.clone()])
                .is_err()
        );
        assert!(SemanticCandidateRanking::new("".into(), "query", vec![pair]).is_err());
        assert!(
            SemanticCandidateRanking::new("m@r".into(), "query", vec![("a".into(), "bad".into())])
                .is_err()
        );
        assert!(
            SemanticCandidateRanking::new(
                "m@r".into(),
                "query",
                (0..101).map(|n| (n.to_string(), "a".repeat(64))).collect()
            )
            .is_err()
        );
    }

    #[test]
    fn each_channel_is_validated_and_resolved_against_the_same_stored_text() {
        let original = evidence();
        let fingerprint = format!("{:x}", Sha256::digest(original.text.as_bytes()));
        let pair = ("entry:a".into(), fingerprint);
        let dense = SemanticCandidateRanking::new("m@r".into(), "q", vec![pair.clone()])
            .expect("valid test fixture");
        assert!(
            dense
                .clone()
                .with_lexical_candidates(vec![pair.clone(), pair.clone()])
                .is_err()
        );
        assert!(
            dense
                .clone()
                .with_lexical_candidates(vec![("x".into(), "bad".into())])
                .is_err()
        );
        assert!(
            dense
                .clone()
                .with_lexical_candidates(
                    (0..101).map(|n| (n.to_string(), "a".repeat(64))).collect()
                )
                .is_err()
        );
        let ranking = dense
            .with_lexical_candidates(vec![pair])
            .expect("valid test fixture");
        assert_eq!(
            ranking.len(),
            1,
            "same ref and text is one proposed identity"
        );
        let channels = ranking.resolve_channels(std::slice::from_ref(&original));
        assert_eq!(channels.len(), 2);
        for (items, channel) in channels.iter().zip(["dense", "bm25"]) {
            assert_eq!(items[0].text, original.text);
            assert_eq!(items[0].source, original.source);
            assert_eq!(items[0].metadata["retrieval_channel"], channel);
            assert_eq!(items[0].metadata["reached_by"], "semantic");
        }
        let mut changed = original;
        changed.text = "The launch was not postponed.".into();
        assert!(
            ranking
                .resolve_channels(&[changed])
                .iter()
                .all(Vec::is_empty)
        );
    }
}
