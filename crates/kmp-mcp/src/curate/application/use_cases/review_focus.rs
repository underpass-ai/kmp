use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::focus_plan::related_request;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::pair_request;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{NONE, PARTNER_AT};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// Most partners Jev proposes for one focused fact.
const PARTNERS_PER_FACT: usize = 3;

/// The relations a few facts are missing, typically the ones just written:
/// kernel pairs that touch them, and the facts of their about Jev reads as
/// directly related, each typed. Undeclared pairs only. Writes nothing.
pub(crate) struct ReviewFocus<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
}

impl ReviewFocus<'_> {
    pub(crate) async fn run(
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
        let mut usage = JevUsage {
            model: model.model().to_string(),
            requests: 0,
            input_tokens: 0,
        };
        let (request, keys) = related_request(&material, focus);
        if !keys.is_empty() {
            match model.evaluate(&request).await {
                Ok(response) => {
                    usage.requests += response.requests;
                    usage.input_tokens += response.input_tokens;
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
                    usage.requests += typed.requests;
                    usage.input_tokens += typed.input_tokens;
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
    use crate::curate::domain::{curate_fact::CurateFact, declared_link::DeclaredLink};

    fn material() -> CurateMaterial {
        let fact = |reference: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
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
        let review = ReviewFocus { judgement: None }
            .run(material(), &["c".to_string()], 12)
            .await;
        assert_eq!(review.findings.len(), 1);
        assert!(review.warnings.iter().any(|w| w.contains("Jev")));
        let unrelated = ReviewFocus { judgement: None }
            .run(material(), &["e".to_string()], 12)
            .await;
        assert!(unrelated.findings.is_empty());
    }
}
