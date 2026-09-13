//! Informational developer control; no CI latency thresholds.
use super::answer_ranker::AnswerEvidenceRanker;
use super::association_index::AssociationIndex;
use super::lexical_field::LexicalField;
use super::lexical_index_cache::LexicalIndexCache;
use super::lexical_index_identity::LexicalIndexIdentity;
use super::lexical_index_tests::{fixture, prepared};
use super::memory_lifecycle::MemoryLifecycle;
use super::{AskRetrievalContext, LexicalBridge, ask_response_from_result};
use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use std::{hint::black_box, sync::Arc, time::Instant};

#[test]
#[ignore = "informational performance measurement; run serially with KMP_LEXICAL_BENCH_OUT"]
fn lexical_index_phase_control() {
    let output = std::env::var("KMP_LEXICAL_BENCH_OUT").expect("set output path");
    let mut rows = Vec::new();
    for (count, vocabulary) in [(8, 4), (64, 16), (256, 64), (32, 256)] {
        let result = fixture(count, vocabulary);
        let start = Instant::now();
        let terms = prepared(&result);
        let preparation_us = start.elapsed().as_micros();
        assert!(!terms.is_empty());
        let identity =
            LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier).expect("revision");
        let cache = Arc::new(LexicalIndexCache::default());
        let start = Instant::now();
        let first = cache.collection(Some(&identity), &terms);
        let first_build_us = start.elapsed().as_micros();
        assert!(
            Arc::ptr_eq(&first, &cache.collection(Some(&identity), &terms)),
            "fixture must hit cache"
        );
        let bridge = LexicalBridge::none();
        let ranker = AnswerEvidenceRanker::from_bundle_at(
            &result.bundle,
            &bridge,
            MemoryLifecycle::read(&result.bundle),
        );
        let cached_ranker = AnswerEvidenceRanker::from_bundle_at(
            &result.bundle,
            &bridge,
            MemoryLifecycle::read(&result.bundle),
        )
        .with_lexical_cache(Some(&cache), Some(identity.clone()));
        let evidence: Vec<_> = terms.iter().map(|(item, _)| item.clone()).collect();
        let mut samples = Vec::new();
        for n in 0..23 {
            let question = if n % 2 == 0 {
                "cache valkey rollout"
            } else {
                "supplier invoice"
            };
            let mut row = serde_json::Map::new();
            for operation in if n % 2 == 0 {
                ["association", "bm25", "hit", "fresh_rank", "cached_rank"]
            } else {
                ["cached_rank", "fresh_rank", "hit", "bm25", "association"]
            } {
                let start = Instant::now();
                match operation {
                    "association" => {
                        black_box(AssociationIndex::build(
                            terms.iter().map(|(_, t)| &t.direct_counts),
                        ));
                    }
                    "bm25" => {
                        black_box(LexicalField::build(
                            terms.iter().map(|(_, t)| &t.content_counts),
                        ));
                        black_box(LexicalField::build(
                            terms.iter().map(|(_, t)| &t.direct_counts),
                        ));
                    }
                    "hit" => {
                        black_box(cache.collection(Some(&identity), &terms));
                    }
                    "fresh_rank" => {
                        black_box(ranker.rank(
                            question,
                            MemoryAnswerPolicy::EvidenceOrUnknown,
                            evidence.clone(),
                        ));
                    }
                    "cached_rank" => {
                        black_box(cached_ranker.rank(
                            question,
                            MemoryAnswerPolicy::EvidenceOrUnknown,
                            evidence.clone(),
                        ));
                    }
                    _ => unreachable!(),
                }
                row.insert(
                    operation.into(),
                    serde_json::json!(start.elapsed().as_micros()),
                );
            }
            assert_eq!(
                ranker.rank(
                    question,
                    MemoryAnswerPolicy::EvidenceOrUnknown,
                    evidence.clone()
                ),
                cached_ranker.rank(
                    question,
                    MemoryAnswerPolicy::EvidenceOrUnknown,
                    evidence.clone()
                )
            );
            if n >= 3 {
                samples.push(row);
            }
        }
        let fresh = ask_response_from_result(
            "cache valkey rollout",
            None,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            None,
            result.clone(),
            &bridge,
            &TemporalSelection::Frontier,
        )
        .expect("fresh");
        let cached = ask_response_from_result(
            "cache valkey rollout",
            None,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            None,
            AskRetrievalContext::from(result).with_lexical_cache(cache),
            &bridge,
            &TemporalSelection::Frontier,
        )
        .expect("cached");
        assert_eq!(fresh, cached);
        rows.push(serde_json::json!({"entries":count,"extra_vocabulary_per_entry":vocabulary,"candidate_count":terms.len(),"preparation_us":preparation_us,"first_build_us":first_build_us,"retained_allowance_bytes":first.retained_bytes(),"samples_us":samples}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&rows).expect("JSON"))
        .expect("write measurements");
}
