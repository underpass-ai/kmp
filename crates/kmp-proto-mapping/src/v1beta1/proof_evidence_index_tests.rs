use std::collections::BTreeSet;
use std::time::Instant;

use kmp_proto::v1beta1::{MemoryEvidence, MemoryRelation, MemorySemanticClass};

use super::ProofEvidenceIndex;

/// The linear scan the index replaced, kept verbatim as the oracle.
fn linear_reference(relation: &MemoryRelation, evidence: &[MemoryEvidence]) -> MemoryRelation {
    let mut relation = relation.clone();
    let mut refs = relation
        .evidence_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut repeated_why = false;
    let mut repeated_evidence = false;
    for item in evidence {
        let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        let why_matches = !relation.why.is_empty() && relation.why == item.text;
        let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;
        let endpoint =
            relation.source_ref == evidence_node_ref || relation.target_ref == evidence_node_ref;
        let supports_endpoint = item.supports.iter().any(|supported_ref| {
            relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
        });
        let incident = endpoint || (supports_endpoint && (why_matches || evidence_matches));
        if incident {
            refs.insert(item.id.clone());
            repeated_why |= why_matches;
            repeated_evidence |= evidence_matches;
        }
    }
    if repeated_why {
        relation.why.clear();
    }
    if repeated_evidence {
        relation.evidence.clear();
    }
    relation.evidence_refs = refs.into_iter().collect();
    if relation.semantic_class != MemorySemanticClass::Structural as i32
        && relation.why.is_empty()
        && relation.evidence.is_empty()
        && !relation.evidence_refs.is_empty()
    {
        relation.why = "Supported by canonical evidence refs.".to_string();
    }
    relation
}

/// Small deterministic generator; the crate carries no `rand`.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

fn node(rng: &mut Lcg, nodes: u64) -> String {
    format!("n{}", rng.below(nodes))
}

fn text(rng: &mut Lcg, texts: u64) -> String {
    match rng.below(texts + 1) {
        0 => String::new(),
        value => format!("body {value}"),
    }
}

fn evidence(rng: &mut Lcg, count: usize, nodes: u64, texts: u64) -> Vec<MemoryEvidence> {
    (0..count)
        .map(|index| {
            let id = match rng.below(4) {
                0 => format!("entry:{}", node(rng, nodes)),
                1 => node(rng, nodes),
                _ => format!("detail:{}", node(rng, nodes)),
            };
            let supports = (0..rng.below(4)).map(|_| node(rng, nodes)).collect();
            MemoryEvidence {
                id: if index % 17 == 0 {
                    // Duplicate ids must still be visited once per item.
                    "detail:n0".to_string()
                } else {
                    id
                },
                supports,
                text: text(rng, texts),
                ..MemoryEvidence::default()
            }
        })
        .collect()
}

fn relations(rng: &mut Lcg, count: usize, nodes: u64, texts: u64) -> Vec<MemoryRelation> {
    (0..count)
        .map(|_| MemoryRelation {
            source_ref: node(rng, nodes),
            target_ref: if rng.below(10) == 0 {
                "n0".to_string()
            } else {
                node(rng, nodes)
            },
            why: text(rng, texts),
            evidence: text(rng, texts),
            semantic_class: if rng.below(3) == 0 {
                MemorySemanticClass::Structural as i32
            } else {
                MemorySemanticClass::Causal as i32
            },
            evidence_refs: (0..rng.below(2))
                .map(|_| format!("detail:{}", node(rng, nodes)))
                .collect(),
            ..MemoryRelation::default()
        })
        .collect()
}

#[test]
fn indexed_normalization_matches_the_linear_scan() {
    for seed in 0..40 {
        let mut rng = Lcg(seed);
        let nodes = 3 + rng.below(40);
        let texts = 1 + rng.below(6);
        let count = 1 + rng.below(80) as usize;
        let evidence = evidence(&mut rng, count, nodes, texts);
        let index = ProofEvidenceIndex::new(&evidence);
        for relation in relations(&mut rng, 60, nodes, texts) {
            let mut indexed = relation.clone();
            index.normalize(&mut indexed);
            assert_eq!(
                indexed,
                linear_reference(&relation, &evidence),
                "seed {seed}"
            );
        }
    }
}

#[test]
fn indexed_normalization_does_not_depend_on_evidence_order() {
    let mut rng = Lcg(7);
    let evidence = evidence(&mut rng, 64, 12, 3);
    let mut reversed = evidence.clone();
    reversed.reverse();
    let forward = ProofEvidenceIndex::new(&evidence);
    let backward = ProofEvidenceIndex::new(&reversed);
    for relation in relations(&mut rng, 64, 12, 3) {
        let (mut left, mut right) = (relation.clone(), relation);
        forward.normalize(&mut left);
        backward.normalize(&mut right);
        assert_eq!(left, right);
    }
}

#[test]
fn a_supporting_source_joins_a_hop_only_through_the_hop_text() {
    let evidence = vec![MemoryEvidence {
        id: "detail:source".to_string(),
        supports: vec!["claim".to_string()],
        text: "measured".to_string(),
        ..MemoryEvidence::default()
    }];
    let index = ProofEvidenceIndex::new(&evidence);
    let hop = |why: &str| MemoryRelation {
        source_ref: "claim".to_string(),
        target_ref: "other".to_string(),
        why: why.to_string(),
        semantic_class: MemorySemanticClass::Causal as i32,
        ..MemoryRelation::default()
    };

    let mut unrelated = hop("because");
    index.normalize(&mut unrelated);
    assert!(unrelated.evidence_refs.is_empty());
    assert_eq!(unrelated.why, "because");

    let mut repeated = hop("measured");
    index.normalize(&mut repeated);
    assert_eq!(repeated.evidence_refs, vec!["detail:source".to_string()]);
    assert_eq!(repeated.why, ProofEvidenceIndex::CANONICAL_REFS_WHY);
}

/// Informational: normalising a proof whose relations and evidence both grow
/// with N. The linear scan was N²; the index must stay ~linear. Run with
/// `cargo test -p kmp-proto-mapping --release proof_normalization_scale -- --ignored --nocapture`.
#[test]
#[ignore = "informational scale measurement"]
fn proof_normalization_scale() {
    let mut rows = Vec::new();
    for count in [1_000_usize, 4_000, 16_000] {
        let nodes = count as u64;
        let evidence = (0..count)
            .map(|index| MemoryEvidence {
                id: format!("detail:n{index}"),
                supports: vec![format!("n{index}")],
                text: format!("Run log line {index}."),
                ..MemoryEvidence::default()
            })
            .collect::<Vec<_>>();
        let mut rng = Lcg(count as u64);
        let path = (0..count * 2)
            .map(|index| MemoryRelation {
                source_ref: node(&mut rng, nodes),
                target_ref: node(&mut rng, nodes),
                why: format!("Step {index}."),
                evidence: format!("Run log line {}.", rng.below(nodes)),
                semantic_class: MemorySemanticClass::Causal as i32,
                ..MemoryRelation::default()
            })
            .collect::<Vec<_>>();
        let start = Instant::now();
        let index = ProofEvidenceIndex::new(&evidence);
        let mut normalized = path.clone();
        for relation in &mut normalized {
            index.normalize(relation);
        }
        let indexed_ms = start.elapsed().as_secs_f64() * 1e3;
        let linear_ms = (count <= 4_000).then(|| {
            let start = Instant::now();
            let reference = path
                .iter()
                .map(|relation| linear_reference(relation, &evidence))
                .collect::<Vec<_>>();
            assert_eq!(reference, normalized);
            start.elapsed().as_secs_f64() * 1e3
        });
        rows.push((count, indexed_ms, linear_ms));
    }
    for (count, indexed_ms, linear_ms) in &rows {
        println!("n={count} indexed_ms={indexed_ms:.2} linear_ms={linear_ms:?}");
    }
    let (_, small, _) = rows[1];
    let (_, large, _) = rows[2];
    // 4× the input: linear is ~4×, the old scan was ~16×.
    assert!(large < small * 10.0 + 5.0, "{rows:?}");
}
