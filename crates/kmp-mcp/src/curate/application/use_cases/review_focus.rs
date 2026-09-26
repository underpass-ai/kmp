use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::focus_plan::related_request;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::pair_request;
use crate::curate::application::lifecycle_plan::{
    PROPOSED_REL, lifecycle_candidates, lifecycle_request, proposed_by_choice,
};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{NONE, PARTNER_AT};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::lifecycle_mode::LifecycleMode;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// Most partners Jev proposes for one focused fact.
const PARTNERS_PER_FACT: usize = 3;

/// The relations a few facts are missing, typically the ones just written:
/// kernel pairs that touch them, the shortlisted facts of their about Jev
/// reads as directly related, each typed, and the current facts they may
/// replace (same principal anchor and entry kind). Undeclared pairs only.
/// Writes nothing.
pub(crate) struct ReviewFocus<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
    /// Whether and how lifecycle pairs are proposed
    /// (`write-relations.json` `lifecycle`).
    pub lifecycle: LifecycleMode,
}

impl ReviewFocus<'_> {
    pub(crate) async fn run(
        &self,
        material: CurateMaterial,
        focus: &[String],
        max_pairs: usize,
    ) -> CurateReview {
        if self.lifecycle == LifecycleMode::Off {
            return self.review(material, focus, max_pairs).await;
        }
        let lifecycle = lifecycle_candidates(&material, focus);
        let mut review = self.review(material.clone(), focus, max_pairs).await;
        self.propose_lifecycle(&mut review, &material, lifecycle)
            .await;
        review
    }

    /// Appends the lifecycle pairs no other finding already proposes: as
    /// `supersedes` for the writer to confirm or retype (`rule`), or as Jev
    /// reads them (`jev`), dropping the ones it reads as novel.
    async fn propose_lifecycle(
        &self,
        review: &mut CurateReview,
        material: &CurateMaterial,
        lifecycle: Vec<CandidatePair>,
    ) {
        let proposed = |pair: &CandidatePair| {
            review.findings.iter().any(|finding| match finding {
                CurateFinding::Missing { pair: other, .. } => {
                    (other.from == pair.from && other.to == pair.to)
                        || (other.from == pair.to && other.to == pair.from)
                }
                CurateFinding::Suspect { .. } => false,
            })
        };
        let fresh = lifecycle
            .into_iter()
            .filter(|pair| !proposed(pair))
            .collect::<Vec<_>>();
        if fresh.is_empty() {
            return;
        }
        let judged = match (self.lifecycle, self.judgement) {
            (LifecycleMode::Jev, Some(model)) => {
                match model.evaluate(&lifecycle_request(material, &fresh)).await {
                    Ok(response) => {
                        let mut usage = review
                            .jev
                            .take()
                            .unwrap_or_else(|| JevUsage::new(model.model()));
                        usage.add(&response);
                        review.jev = Some(usage);
                        Some(response)
                    }
                    Err(error) => {
                        review.warnings.push(format!(
                            "Jev could not read the lifecycle pairs; proposed unread: {error}"
                        ));
                        None
                    }
                }
            }
            _ => None,
        };
        for (n, pair) in fresh.into_iter().enumerate() {
            let finding = match judged
                .as_ref()
                .and_then(|response| response.answers.get(&format!("l{n}")))
            {
                Some(JudgementAnswer::Choice {
                    choice,
                    probabilities,
                    confidence,
                }) => match proposed_by_choice(choice) {
                    Some(rel) => CurateFinding::Missing {
                        pair,
                        suggested_rel: Some(rel.to_string()),
                        verdict: Some(JevVerdict {
                            choice: choice.clone(),
                            probabilities: probabilities.clone(),
                            confidence: *confidence,
                        }),
                    },
                    None => continue,
                },
                _ => CurateFinding::Missing {
                    pair,
                    suggested_rel: Some(PROPOSED_REL.to_string()),
                    verdict: None,
                },
            };
            review.findings.push(finding);
        }
    }

    async fn review(
        &self,
        material: CurateMaterial,
        focus: &[String],
        max_pairs: usize,
    ) -> CurateReview {
        let mut review = CurateReview {
            findings: Vec::new(),
            jev: None,
            warnings: Vec::new(),
            selection: format!("{}|focus:{}", material.selection, focus.join(",")),
        };
        for missing in focus.iter().filter(|r| material.fact(r).is_none()) {
            review.warnings.push(format!(
                "`{missing}` is not a current fact of the selection"
            ));
        }
        let declared = |a: &str, b: &str| {
            material
                .declared
                .iter()
                .any(|link| (link.from == a && link.to == b) || (link.from == b && link.to == a))
        };
        let same = |left: &CandidatePair, from: &str, to: &str| {
            (left.from == from && left.to == to) || (left.from == to && left.to == from)
        };
        let mut pairs = material
            .pairs
            .iter()
            .filter(|pair| focus.contains(&pair.from) || focus.contains(&pair.to))
            .filter(|pair| !declared(&pair.from, &pair.to))
            .cloned()
            .collect::<Vec<_>>();
        let Some(model) = self.judgement else {
            review.warnings.push(
                "Jev is not configured for this store; only kernel pairs are returned, untyped"
                    .into(),
            );
            review.findings = pairs
                .into_iter()
                .take(max_pairs)
                .map(|pair| CurateFinding::Missing {
                    pair,
                    suggested_rel: None,
                    verdict: None,
                })
                .collect();
            return review;
        };
        let mut usage = JevUsage::new(model.model());
        let (request, keys) = related_request(&material, focus);
        if !keys.is_empty() {
            match model.evaluate(&request).await {
                Ok(response) => {
                    usage.add(&response);
                    for focused in focus {
                        let mut related = keys
                            .iter()
                            .filter(|(_, (own, _))| own == focused)
                            .filter_map(|(key, (_, other))| match response.answers.get(key) {
                                Some(JudgementAnswer::Noul { yes }) if *yes >= PARTNER_AT => {
                                    Some((*yes, other.clone()))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        related.sort_by(|left, right| {
                            right
                                .0
                                .total_cmp(&left.0)
                                .then_with(|| left.1.cmp(&right.1))
                        });
                        let fresh = related
                            .into_iter()
                            .map(|(_, other)| other)
                            .filter(|other| {
                                !declared(focused, other)
                                    && !pairs.iter().any(|pair| same(pair, focused, other))
                            })
                            .take(PARTNERS_PER_FACT)
                            .collect::<Vec<_>>();
                        for other in fresh {
                            pairs.push(CandidatePair {
                                from: focused.clone(),
                                to: other,
                                origin: PairOrigin::Jev,
                                crosses_abouts: false,
                            });
                        }
                    }
                }
                Err(error) => review
                    .warnings
                    .push(format!("Jev unavailable; kernel pairs only: {error}")),
            }
        }
        if !pairs.is_empty() {
            match model.evaluate(&pair_request(&material, &pairs)).await {
                Ok(typed) => {
                    usage.add(&typed);
                    let mut missing = pairs
                        .into_iter()
                        .enumerate()
                        .filter_map(|(n, pair)| match typed.answers.get(&format!("t{n}")) {
                            Some(JudgementAnswer::Choice {
                                choice,
                                probabilities,
                                confidence,
                            }) if choice != NONE => Some(CurateFinding::Missing {
                                pair,
                                suggested_rel: Some(choice.clone()),
                                verdict: Some(JevVerdict {
                                    choice: choice.clone(),
                                    probabilities: probabilities.clone(),
                                    confidence: *confidence,
                                }),
                            }),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    missing.sort_by(|left, right| confidence(right).total_cmp(&confidence(left)));
                    missing.truncate(max_pairs);
                    review.findings = missing;
                }
                Err(error) => review
                    .warnings
                    .push(format!("Jev could not type the pairs: {error}")),
            }
        }
        review.jev = Some(usage);
        review
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use super::super::scripted_judgement::Scripted;
    use crate::curate::domain::pair_origin::PairOrigin;
    use crate::curate::domain::{curate_fact::CurateFact, declared_link::DeclaredLink};

    fn material() -> CurateMaterial {
        let fact = |reference: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
            kind: String::new(),
            text: format!("text {reference}"),
            occurred: None,
            labels: Vec::new(),
        };
        CurateMaterial {
            facts: ["new", "b", "c", "d", "e"].into_iter().map(fact).collect(),
            declared: vec![DeclaredLink {
                from: "new".into(),
                to: "b".into(),
                rel: "supports".into(),
                why: "w".into(),
                evidence: "e".into(),
            }],
            pairs: vec![CandidatePair {
                from: "c".into(),
                to: "d".into(),
                origin: PairOrigin::Kernel {
                    signals: vec!["entity".into()],
                    why: "both name it".into(),
                },
                crosses_abouts: false,
            }],
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[tokio::test]
    async fn jev_proposes_typed_partners_for_the_focus_only_and_skips_declared_ones() {
        let model = Scripted {
            noul: 0.9,
            choice: "supports",
            confidence: 0.8,
            calls: Mutex::new(0),
        };
        let review = ReviewFocus {
            judgement: Some(&model),
            lifecycle: LifecycleMode::Off,
        }
        .run(material(), &["new".to_string()], 12)
        .await;
        let pairs = review
            .findings
            .iter()
            .map(|finding| match finding {
                CurateFinding::Missing { pair, .. } => (pair.from.clone(), pair.to.clone()),
                CurateFinding::Suspect { .. } => panic!("a focused review audits nothing"),
            })
            .collect::<Vec<_>>();
        assert_eq!(pairs.len(), PARTNERS_PER_FACT, "{pairs:?}");
        assert!(pairs.iter().all(|(from, _)| from == "new"), "{pairs:?}");
        assert!(
            !pairs.iter().any(|(_, to)| to == "b"),
            "already declared: {pairs:?}"
        );
        assert_eq!(review.jev.as_ref().map(|usage| usage.requests), Some(2));
    }

    #[tokio::test]
    async fn without_jev_only_kernel_pairs_that_touch_the_focus_come_back() {
        let review = ReviewFocus {
            judgement: None,
            lifecycle: LifecycleMode::Off,
        }
        .run(material(), &["c".to_string()], 12)
        .await;
        assert_eq!(review.findings.len(), 1);
        assert!(review.warnings.iter().any(|w| w.contains("Jev")));
        let unrelated = ReviewFocus {
            judgement: None,
            lifecycle: LifecycleMode::Off,
        }
        .run(material(), &["e".to_string()], 12)
        .await;
        assert!(unrelated.findings.is_empty());
    }

    fn lifecycle_material() -> CurateMaterial {
        let fact = |reference: &str, kind: &str, text: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
            kind: kind.into(),
            text: text.into(),
            occurred: None,
            labels: Vec::new(),
        };
        CurateMaterial {
            facts: vec![
                fact("old", "decision", "Deploy 2.4.1 is scheduled for Friday."),
                fact("far", "decision", "Invoices are generated as PDF."),
                fact("new", "decision", "Deploy 2.4.1 moves to Monday."),
            ],
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    fn lifecycle_findings(review: &CurateReview) -> Vec<(String, Option<String>)> {
        review
            .findings
            .iter()
            .filter_map(|finding| match finding {
                CurateFinding::Missing {
                    pair,
                    suggested_rel,
                    ..
                } if matches!(pair.origin, PairOrigin::Lifecycle { .. }) => {
                    Some((pair.to.clone(), suggested_rel.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn lifecycle_pairs_are_proposed_only_when_the_store_asks() {
        let focus = ["new".to_string()];
        let off = ReviewFocus {
            judgement: None,
            lifecycle: LifecycleMode::Off,
        }
        .run(lifecycle_material(), &focus, 12)
        .await;
        assert!(lifecycle_findings(&off).is_empty());
        let rule = ReviewFocus {
            judgement: None,
            lifecycle: LifecycleMode::Rule,
        }
        .run(lifecycle_material(), &focus, 12)
        .await;
        assert_eq!(
            lifecycle_findings(&rule),
            [("old".to_string(), Some("supersedes".to_string()))]
        );
    }

    #[tokio::test]
    async fn under_jev_a_lifecycle_pair_is_retyped_or_withdrawn_by_its_reading() {
        let run = |choice: &'static str| async move {
            let focus = ["new".to_string()];
            let model = Scripted {
                noul: 0.0,
                choice,
                confidence: 0.9,
                calls: Mutex::new(0),
            };
            let review = ReviewFocus {
                judgement: Some(&model),
                lifecycle: LifecycleMode::Jev,
            }
            .run(lifecycle_material(), &focus, 12)
            .await;
            let calls = *model.calls.lock().expect("calls");
            (lifecycle_findings(&review), calls)
        };
        let (updated, calls) = run("update_state").await;
        assert_eq!(
            updated,
            [("old".to_string(), Some("updates_state".to_string()))]
        );
        assert_eq!(calls, 2, "the partner round and the lifecycle reading");
        let (novel, _) = run("novel").await;
        assert!(novel.is_empty(), "a novel pair is withdrawn");
    }
}
