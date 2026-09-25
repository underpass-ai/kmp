use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{
    pair_request, partner_request, relation_options, suspect_request,
};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{DOUBT_BELOW, NONE, PARTNER_AT, RETYPE_AT};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Review the relations of a reading: kernel pairs, Jev partners for
/// orphans, a type for every pair, contradictions nobody declared, and
/// declared links whose reason does not hold. Writes nothing.
pub(crate) struct ReviewRelations<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
}

impl ReviewRelations<'_> {
    pub(crate) async fn run(&self, material: CurateMaterial, max_pairs: usize) -> CurateReview {
        let mut review = CurateReview {
            findings: Vec::new(),
            jev: None,
            warnings: Vec::new(),
            selection: material.selection.clone(),
        };
        let Some(model) = self.judgement else {
            review.warnings.push(
                "Jev is not configured for this store; kernel pairs are returned untyped and declared relations were not audited".into(),
            );
            review.findings = untyped(&material.pairs, max_pairs);
            return review;
        };
        let mut usage = JevUsage {
            model: model.model().to_string(),
            requests: 0,
            input_tokens: 0,
        };
        let mut ask = async |request: JudgementRequest| -> Option<JudgementResponse> {
            match model.evaluate(&request).await {
                Ok(response) => {
                    usage.requests += response.requests;
                    usage.input_tokens += response.input_tokens;
                    Some(response)
                }
                Err(error) => {
                    review
                        .warnings
                        .push(format!("Jev unavailable; using kernel pairs only: {error}"));
                    None
                }
            }
        };

        let mut pairs = material.pairs.clone();
        for about in abouts_with_orphans(&material) {
            let facts = material
                .facts
                .iter()
                .filter(|f| f.about == about)
                .collect::<Vec<_>>();
            let orphans = material
                .orphans()
                .into_iter()
                .filter(|f| f.about == about)
                .collect::<Vec<_>>();
            let Some((request, keys)) = partner_request(&facts, &orphans) else {
                continue;
            };
            let Some(response) = ask(request).await else {
                break;
            };
            for (own, answer) in &response.answers {
                if let JudgementAnswer::Choice {
                    choice, confidence, ..
                } = answer
                    && choice != NONE
                    && *confidence >= PARTNER_AT
                    && let (Some(from), Some(to)) = (keys.get(own), keys.get(choice))
                {
                    pairs.push(CandidatePair {
                        from: from.clone(),
                        to: to.clone(),
                        origin: PairOrigin::Jev,
                        crosses_abouts: false,
                    });
                }
            }
        }

        let typed = if pairs.is_empty() {
            Some(JudgementResponse::empty(&usage.model))
        } else {
            ask(pair_request(&material, &pairs)).await
        };
        let Some(typed) = typed else {
            review.findings = untyped(&material.pairs, max_pairs);
            return review;
        };
        let mut missing = Vec::new();
        for (n, pair) in pairs.into_iter().enumerate() {
            match typed.answers.get(&format!("t{n}")).and_then(verdict_of) {
                Some(verdict) if verdict.choice != NONE => missing.push(CurateFinding::Missing {
                    pair,
                    suggested_rel: Some(verdict.choice.clone()),
                    verdict: Some(verdict),
                }),
                _ => continue,
            }
        }
        missing.sort_by(|left, right| confidence(right).total_cmp(&confidence(left)));
        missing.truncate(max_pairs);
        review.findings = missing;

        if !material.declared.is_empty()
            && let Some(audit) = ask(suspect_request(&material)).await
        {
            for (n, link) in material.declared.iter().enumerate() {
                let support = match audit.answers.get(&format!("s{n}")) {
                    Some(JudgementAnswer::Noul { yes }) => *yes,
                    _ => continue,
                };
                let Some(best) = audit.answers.get(&format!("b{n}")).and_then(verdict_of) else {
                    continue;
                };
                // A stored type Jev was not offered (legacy or kernel-written)
                // cannot be matched by its choice; judge it on support alone.
                let crosses = material.fact(&link.from).map(|fact| &fact.about)
                    != material.fact(&link.to).map(|fact| &fact.about);
                let offered = relation_options(crosses).contains(&link.rel);
                let direction = audit.answers.get(&format!("d{n}")).and_then(direction_of);
                let reasons = doubt_reasons(support, &best, &link.rel, offered, direction);
                if !reasons.is_empty() {
                    review.findings.push(CurateFinding::Suspect {
                        link: link.clone(),
                        support,
                        best,
                        direction,
                        reasons,
                    });
                }
            }
        }
        review.jev = Some(usage);
        review
    }
}

/// How strongly the judge reads the relation the declared way round:
/// forward over forward plus backward. None when it reads neither way, which
/// is a matter for support, not direction.
pub(crate) fn direction_of(answer: &JudgementAnswer) -> Option<f64> {
    let JudgementAnswer::Choice { probabilities, .. } = answer else {
        return None;
    };
    let forward = probabilities.get("forward").copied().unwrap_or(0.0);
    let backward = probabilities.get("backward").copied().unwrap_or(0.0);
    (forward + backward >= 0.2).then(|| forward / (forward + backward))
}

/// Why a declaration, or an item about to be written, is doubted: its reason
/// does not hold, Jev would type it otherwise, or it runs the wrong way.
pub(crate) fn doubt_reasons(
    support: f64,
    best: &JevVerdict,
    rel: &str,
    offered: bool,
    direction: Option<f64>,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if support < DOUBT_BELOW {
        reasons.push("support");
    }
    if offered && best.choice != rel && best.choice != NONE && best.confidence >= RETYPE_AT {
        reasons.push("type");
    }
    if direction.is_some_and(|direction| direction < DOUBT_BELOW) {
        reasons.push("direction");
    }
    reasons
}

fn verdict_of(answer: &JudgementAnswer) -> Option<JevVerdict> {
    match answer {
        JudgementAnswer::Choice {
            choice,
            probabilities,
            confidence,
        } => Some(JevVerdict {
            choice: choice.clone(),
            probabilities: probabilities.clone(),
            confidence: *confidence,
        }),
        JudgementAnswer::Noul { .. } => None,
    }
}

fn confidence(finding: &CurateFinding) -> f64 {
    match finding {
        CurateFinding::Missing {
            verdict: Some(verdict),
            ..
        } => verdict.confidence,
        _ => 0.0,
    }
}

fn untyped(pairs: &[CandidatePair], max_pairs: usize) -> Vec<CurateFinding> {
    pairs
        .iter()
        .take(max_pairs)
        .cloned()
        .map(|pair| CurateFinding::Missing {
            pair,
            suggested_rel: None,
            verdict: None,
        })
        .collect()
}

fn abouts_with_orphans(material: &CurateMaterial) -> Vec<String> {
    let mut abouts = material
        .orphans()
        .iter()
        .map(|f| f.about.clone())
        .collect::<Vec<_>>();
    abouts.sort();
    abouts.dedup();
    abouts
}

#[cfg(test)]
mod tests {
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
            text: format!("text {reference}"),
            occurred: None,
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
        }
    }

    #[tokio::test]
    async fn without_jev_kernel_pairs_come_back_untyped_with_a_warning() {
        let review = ReviewRelations { judgement: None }
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
        }
        .run(material(), 12)
        .await;
        assert!(
            !review.findings.iter().any(|f| matches!(f, CurateFinding::Missing { suggested_rel: Some(rel), .. } if rel == "none")),
            "none is never proposed"
        );
        let capped = ReviewRelations { judgement: None }.run(material(), 0).await;
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
        let verdict =
            |choice: &str, confidence: f64| crate::curate::domain::jev_verdict::JevVerdict {
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
    fn direction_is_asked_only_of_relations_that_have_one() {
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
            request.questions.contains_key("d0"),
            "causes has a direction"
        );
        assert!(
            !request.questions.contains_key("d1"),
            "same_event_as reads both ways"
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
}
