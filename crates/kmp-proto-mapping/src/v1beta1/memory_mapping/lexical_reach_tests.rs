use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{BundleRelationship, RelationExplanation, RelationSemanticClass};
use kmp_proto::v1beta1::MemoryEvidence;

use super::LexicalReach;
use crate::v1beta1::memory_mapping::answer_candidate_terms::AnswerCandidateTerms;
use crate::v1beta1::memory_mapping::answer_recall_context::AnswerRecallContext;
use crate::v1beta1::memory_mapping::lexical_profile::LexicalProfile;
use crate::v1beta1::memory_mapping::lexical_row::LexicalRow;
use crate::v1beta1::memory_mapping::reach_relation::ReachRelation;

fn explanation(
    class: RelationSemanticClass,
    evidence: &str,
    rationale: &str,
) -> RelationExplanation {
    let mut properties = BTreeMap::new();
    properties.insert(
        "semantic_class".to_string(),
        format!("{class:?}").to_lowercase(),
    );
    if !evidence.is_empty() {
        properties.insert("evidence".to_string(), evidence.to_string());
    }
    properties.insert("rationale".to_string(), rationale.to_string());
    RelationExplanation::from_properties(&properties).expect("fixture explanation")
}

fn entry(id: &str, text: &str) -> MemoryEvidence {
    MemoryEvidence {
        id: format!("entry:{id}"),
        supports: vec![id.to_string()],
        text: text.to_string(),
        source: id.to_string(),
        ..Default::default()
    }
}

/// What the ranker counts a candidate's focus over, beyond its direct field,
/// is always among the direct field and the reach: the claim (its refs'
/// words and details) and the explanatory relations at its refs.
#[test]
fn the_reach_holds_every_word_claims_and_relations_lend_beyond_the_row() {
    let profile = LexicalProfile::new(None);
    let item = entry("plant:valve-room", "The reserve valve froze overnight.");
    let row = LexicalRow::read(&item, &profile);
    let causal = explanation(
        RelationSemanticClass::Causal,
        "The thermometer read minus twelve.",
        "Frost burst the pipe.",
    );
    let structural = explanation(RelationSemanticClass::Structural, "", "Scope keeps it.");
    let relations = [
        (
            "plant:valve-room",
            "plant:boiler-hall",
            "caused_by",
            &causal,
        ),
        (
            "label:plant",
            "plant:valve-room",
            "contains_entry",
            &structural,
        ),
    ];
    let detail = "Crews replaced the gasket at dawn.";
    let reach = LexicalReach::read(
        &item,
        &row,
        &profile,
        [detail],
        relations.iter().map(
            |(source, target, relation_type, explanation)| ReachRelation {
                source,
                target,
                relation_type,
                explanation,
            },
        ),
    );
    // The ranker, over a bundle holding the same detail and relations.
    let bundle = kmp_domain::KmpBundle::new(
        kmp_domain::CaseId::new("plant").expect("fixture"),
        kmp_domain::Role::new("reader").expect("fixture"),
        kmp_domain::BundleNode::new(
            "plant",
            "memory_anchor",
            "plant",
            "anchor",
            "ACTIVE",
            vec![],
            Default::default(),
        ),
        ["plant:valve-room", "plant:boiler-hall", "label:plant"]
            .into_iter()
            .map(|id| {
                kmp_domain::BundleNode::new(
                    id,
                    "observation",
                    id,
                    "summary",
                    "ACTIVE",
                    vec![],
                    Default::default(),
                )
            })
            .collect(),
        relations
            .iter()
            .map(|(source, target, relation_type, explanation)| {
                BundleRelationship::new(*source, *target, *relation_type, (*explanation).clone())
            })
            .collect(),
        vec![kmp_domain::BundleNodeDetail::new(
            "plant:valve-room",
            detail,
            "hash",
            1,
        )],
        kmp_domain::BundleMetadata::initial("reach-test"),
    )
    .expect("fixture bundle");
    let context = AnswerRecallContext::from_bundle_in(&bundle, Default::default(), None);
    let terms = AnswerCandidateTerms::from_evidence(&item, &context);
    let direct = row.direct_counts(false);
    let bound = direct
        .terms()
        .chain(reach.terms())
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        terms.searchable.is_subset(&bound),
        "missing {:?}",
        terms.searchable.difference(&bound).collect::<Vec<_>>()
    );
    // Words the structural relation carries are not lent; the row's own
    // words are not repeated.
    assert!(!reach.terms().iter().any(|term| term == "scope"));
    assert!(reach.terms().iter().all(|term| direct.count(term) == 0));
    assert_eq!(LexicalReach::decode(&reach.encode()), Ok(reach));
}

#[test]
fn a_reach_of_another_layout_is_refused() {
    assert!(LexicalReach::decode(&[9, 0]).is_err());
    assert!(LexicalReach::decode(&[1, 1]).is_err());
    assert_eq!(
        LexicalReach::decode(&LexicalReach::default().encode()),
        Ok(LexicalReach::default())
    );
}
