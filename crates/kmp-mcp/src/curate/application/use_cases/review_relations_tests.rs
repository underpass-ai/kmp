use super::*;
use std::sync::Mutex;

use super::super::scripted_judgement::Scripted;
use crate::curate::domain::{
    candidate_pair::CandidatePair, curate_fact::CurateFact, declared_link::DeclaredLink,
    pair_origin::PairOrigin,
};

fn fact(reference: &str, about: &str) -> CurateFact {
    CurateFact {
        reference: reference.into(),
        about: about.into(),
        kind: String::new(),
        text: format!("text {reference}"),
        occurred: None,
        labels: Vec::new(),
    }
}

fn material() -> CurateMaterial {
    CurateMaterial {
        facts: vec![
            fact("a1", "a"),
            fact("a2", "a"),
            fact("a3", "a"),
            fact("b1", "b"),
        ],
        declared: vec![DeclaredLink {
            from: "a3".into(),
            to: "a1".into(),
            rel: "causes".into(),
            why: "w".into(),
            evidence: "e".into(),
        }],
        pairs: vec![CandidatePair {
            from: "a1".into(),
            to: "b1".into(),
            origin: PairOrigin::Kernel {
                signals: vec!["entity".into()],
                why: "both name Valkey".into(),
            },
            crosses_abouts: true,
        }],
        selection: "fp".into(),
        past: Vec::new(),
    }
}

#[tokio::test]
async fn without_jev_kernel_pairs_come_back_untyped_with_a_warning() {
    let review = ReviewRelations {
        judgement: None,
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(material(), 12)
    .await;
    assert_eq!(review.findings.len(), 1);
    assert!(matches!(
        &review.findings[0],
        CurateFinding::Missing {
            suggested_rel: None,
            verdict: None,
            ..
        }
    ));
    assert!(review.jev.is_none());
    assert!(review.warnings.iter().any(|w| w.contains("Jev")));
}

#[tokio::test]
async fn jev_types_pairs_finds_partners_and_flags_weak_declarations() {
    let model = Scripted {
        noul: 0.1,
        choice: "same_entity_as",
        confidence: 0.9,
        calls: Mutex::new(0),
    };
    let review = ReviewRelations {
        judgement: Some(&model),
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(material(), 12)
    .await;
    let missing = review
        .findings
        .iter()
        .filter(|f| matches!(f, CurateFinding::Missing { .. }))
        .count();
    let suspect = review
        .findings
        .iter()
        .filter(|f| matches!(f, CurateFinding::Suspect { .. }))
        .count();
    assert!(missing >= 1, "the cross-about pair is typed same_entity_as");
    assert_eq!(suspect, 1, "support 0.1 is below 0.3");
    assert!(matches!(
        review.findings.iter().find(|f| matches!(f, CurateFinding::Missing { pair, .. } if pair.crosses_abouts)),
        Some(CurateFinding::Missing { suggested_rel: Some(rel), .. }) if rel == "same_entity_as"
    ));
    assert_eq!(
        review.jev.as_ref().map(|u| u.model.as_str()),
        Some("jev-test")
    );
    assert!(*model.calls.lock().expect("calls") >= 2);
}

#[tokio::test]
async fn a_pair_typed_none_is_dropped_and_max_pairs_caps_missing() {
    let model = Scripted {
        noul: 0.9,
        choice: "not-offered",
        confidence: 0.9,
        calls: Mutex::new(0),
    };
    let review = ReviewRelations {
        judgement: Some(&model),
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(material(), 12)
    .await;
    assert!(
        !review.findings.iter().any(|f| matches!(f, CurateFinding::Missing { suggested_rel: Some(rel), .. } if rel == "none")),
        "none is never proposed"
    );
    let capped = ReviewRelations {
        judgement: None,
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(material(), 0)
    .await;
    assert!(capped.findings.is_empty());
}

#[test]
fn the_token_binds_selection_and_findings() {
    let base = CurateReview {
        findings: vec![],
        jev: None,
        warnings: vec![],
        selection: "fp".into(),
    };
    let other = CurateReview {
        selection: "fp2".into(),
        ..base.clone()
    };
    assert_ne!(base.token(), other.token());
    assert_eq!(base.token(), base.clone().token());
}

#[tokio::test]
async fn a_status_update_is_not_reported_as_a_contradiction() {
    let model = Scripted {
        noul: 0.95,
        choice: "updates_state",
        confidence: 0.8,
        calls: Mutex::new(0),
    };
    let review = ReviewRelations {
        judgement: Some(&model),
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(material(), 12)
    .await;
    let suggested = review
        .findings
        .iter()
        .filter_map(|f| match f {
            CurateFinding::Missing { suggested_rel, .. } => suggested_rel.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !suggested.iter().any(|rel| rel == "contradicts"),
        "{suggested:?}"
    );
}

#[tokio::test]
async fn past_the_partner_cap_no_partner_choice_is_asked_and_the_review_says_so() {
    let calls = async |cap: Option<&str>| {
        let model = Scripted {
            noul: 0.9,
            choice: "supports",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let partner_cap = cap.and_then(PartnerCap::named).unwrap_or_default();
        let review = ReviewRelations {
            judgement: Some(&model),
            partner_cap,
            partner_filter: PartnerFilter::Off,
        }
        .run(material(), 12)
        .await;
        let calls = *model.calls.lock().expect("calls");
        (calls, review.warnings)
    };
    let (within, quiet) = calls(None).await;
    assert_eq!(within, 3, "partners, typing and audit");
    assert!(quiet.is_empty(), "{quiet:?}");
    let (past, warned) = calls(Some("2")).await;
    assert_eq!(past, 2, "typing and audit only");
    assert!(warned.iter().any(|w| w.contains("at most 2")), "{warned:?}");
}

#[test]
fn structural_types_are_never_offered() {
    let options = crate::curate::application::judgement_plan::relation_options(false);
    for structural in ["contains", "member_of", "scoped_to"] {
        assert!(!options.iter().any(|o| o == structural), "{options:?}");
    }
    assert!(options.iter().any(|o| o == "contradicts"));
    assert!(options.iter().any(|o| o == "supersedes"));
}

#[tokio::test]
async fn with_nothing_to_pair_only_the_audit_is_asked() {
    let model = Scripted {
        noul: 0.9,
        choice: "causes",
        confidence: 0.9,
        calls: Mutex::new(0),
    };
    let mut lone = material();
    lone.pairs.clear();
    lone.facts
        .retain(|fact| fact.reference == "a1" || fact.reference == "a3");
    let review = ReviewRelations {
        judgement: Some(&model),
        partner_cap: PartnerCap::DEFAULT,
        partner_filter: PartnerFilter::Off,
    }
    .run(lone, 12)
    .await;
    assert!(
        review.findings.is_empty(),
        "a type Jev was not offered is judged on support alone: {:?}",
        review.findings
    );
    assert_eq!(
        *model.calls.lock().expect("calls"),
        1,
        "one audit request, no pairing"
    );
    assert_eq!(review.jev.as_ref().map(|usage| usage.requests), Some(1));
}

#[test]
fn each_reason_is_reported_on_its_own() {
    let verdict = |choice: &str, confidence: f64| crate::curate::domain::jev_verdict::JevVerdict {
        choice: choice.into(),
        probabilities: Default::default(),
        confidence,
    };
    assert!(
        doubt_reasons(
            0.9,
            &verdict("supersedes", 0.9),
            "supersedes",
            true,
            Some(0.9)
        )
        .is_empty()
    );
    assert_eq!(
        doubt_reasons(
            0.1,
            &verdict("supersedes", 0.9),
            "supersedes",
            true,
            Some(0.9)
        ),
        vec!["support"]
    );
    assert_eq!(
        doubt_reasons(
            0.9,
            &verdict("supports", 0.8),
            "supersedes",
            true,
            Some(0.9)
        ),
        vec!["type"]
    );
    assert_eq!(
        doubt_reasons(
            0.9,
            &verdict("supersedes", 0.9),
            "supersedes",
            true,
            Some(0.1)
        ),
        vec!["direction"]
    );
    assert!(
        doubt_reasons(0.9, &verdict("none", 0.9), "supersedes", true, None).is_empty(),
        "none is no retype"
    );
    assert!(
        doubt_reasons(0.9, &verdict("supports", 0.9), "causes", false, None).is_empty(),
        "unoffered type"
    );
}

#[test]
fn the_audit_does_not_ask_direction() {
    let mut lone = material();
    lone.declared.push(DeclaredLink {
        from: "a1".into(),
        to: "b1".into(),
        rel: "same_event_as".into(),
        why: "w".into(),
        evidence: "e".into(),
    });
    let request = crate::curate::application::judgement_plan::suspect_request(&lone);
    assert!(
        !request.questions.keys().any(|key| key.starts_with('d')),
        "the audit does not ask direction: it added nothing measured"
    );
}

#[test]
fn direction_is_forward_over_both_ways_and_silent_when_neither() {
    let choice = |forward: f64, backward: f64| JudgementAnswer::Choice {
        choice: "forward".into(),
        probabilities: [
            ("forward".to_string(), forward),
            ("backward".to_string(), backward),
            ("none".to_string(), 1.0 - forward - backward),
        ]
        .into_iter()
        .collect(),
        confidence: 0.9,
    };
    assert_eq!(direction_of(&choice(0.8, 0.2)), Some(0.8));
    assert_eq!(direction_of(&choice(0.1, 0.3)), Some(0.25));
    assert_eq!(
        direction_of(&choice(0.05, 0.05)),
        None,
        "neither is not a direction"
    );
    assert_eq!(direction_of(&JudgementAnswer::Noul { yes: 0.9 }), None);
}
