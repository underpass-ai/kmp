use std::collections::BTreeMap;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{pair_request, partner_request, suspect_request};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{
    CONTRADICTION_AT, DOUBT_BELOW, NONE, PARTNER_AT, RETYPE_AT,
};
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
            let verdict = typed.answers.get(&format!("t{n}")).and_then(verdict_of);
            let clash = match typed.answers.get(&format!("c{n}")) {
                Some(JudgementAnswer::Noul { yes }) => *yes,
                _ => 0.0,
            };
            let (suggested, verdict) = if clash >= CONTRADICTION_AT {
                (
                    "contradicts".to_string(),
                    JevVerdict {
                        choice: "contradicts".into(),
                        probabilities: BTreeMap::from([
                            ("contradicts".into(), clash),
                            (NONE.into(), 1.0 - clash),
                        ]),
                        confidence: clash,
                    },
                )
            } else {
                match verdict {
                    Some(verdict) if verdict.choice != NONE => (verdict.choice.clone(), verdict),
                    _ => continue,
                }
            };
            missing.push(CurateFinding::Missing {
                pair,
                suggested_rel: Some(suggested),
                verdict: Some(verdict),
            });
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
                if support < DOUBT_BELOW
                    || (best.choice != link.rel && best.confidence >= RETYPE_AT)
                {
                    review.findings.push(CurateFinding::Suspect {
                        link: link.clone(),
                        support,
                        best,
                    });
                }
            }
        }
        review.jev = Some(usage);
        review
    }
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
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use crate::curate::domain::{
        candidate_pair::CandidatePair, curate_fact::CurateFact, declared_link::DeclaredLink,
        pair_origin::PairOrigin,
    };
    use crate::serving::judgement_answer::JudgementAnswer;
    use crate::serving::judgement_question::JudgementQuestion;
    use crate::serving::judgement_request::JudgementRequest;
    use crate::serving::judgement_response::JudgementResponse;

    /// Answers every noul with `noul` and every choice with `choice` when
    /// offered (else `none`), at `confidence`.
    struct Scripted {
        noul: f64,
        choice: &'static str,
        confidence: f64,
        calls: Mutex<usize>,
    }

    impl JudgementModel for Scripted {
        fn model(&self) -> &str {
            "jev-test"
        }
        fn evaluate<'a>(
            &'a self,
            request: &'a JudgementRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
        > {
            *self.calls.lock().expect("calls") += 1;
            let answers = request
                .questions
                .iter()
                .map(|(key, question)| {
                    let answer = match question {
                        JudgementQuestion::Noul { .. } => JudgementAnswer::Noul { yes: self.noul },
                        JudgementQuestion::Choice { options, .. } => {
                            let choice = if options.iter().any(|o| o == self.choice) {
                                self.choice
                            } else {
                                "none"
                            };
                            JudgementAnswer::Choice {
                                choice: choice.into(),
                                probabilities: options
                                    .iter()
                                    .map(|o| {
                                        (o.clone(), if o == choice { self.confidence } else { 0.0 })
                                    })
                                    .collect(),
                                confidence: self.confidence,
                            }
                        }
                    };
                    (key.clone(), answer)
                })
                .collect::<BTreeMap<_, _>>();
            Box::pin(async move {
                Ok(JudgementResponse {
                    model: "jev-test".into(),
                    answers,
                    input_tokens: 10,
                    requests: 1,
                })
            })
        }
    }

    fn fact(reference: &str, about: &str) -> CurateFact {
        CurateFact {
            reference: reference.into(),
            about: about.into(),
            text: format!("text {reference}"),
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
}
