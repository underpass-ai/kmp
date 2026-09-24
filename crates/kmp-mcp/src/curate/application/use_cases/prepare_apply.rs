use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{precheck_request, relation_options};
use crate::curate::application::prepared_apply::PreparedApply;
use crate::curate::application::prepared_relation::PreparedRelation;
use crate::curate::domain::apply_doubt::ApplyDoubt;
use crate::curate::domain::apply_item::ApplyItem;
use crate::curate::domain::apply_rejection::ApplyRejection;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{DOUBT_BELOW, NONE, RETYPE_AT};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// The relate signals a proposal across abouts may name.
const PROPOSAL_SIGNALS: [&str; 3] = ["identifier", "summary", "entity"];

/// Resolve what the agent accepted against the frozen review, keep what this
/// call may write, and give Jev one look at the agent's own text. A doubt
/// withholds an item until the agent confirms it; Jev never refuses a write.
pub(crate) struct PrepareApply<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
}

impl PrepareApply<'_> {
    pub(crate) async fn run(
        &self,
        about: &str,
        review: &CurateReview,
        material: &CurateMaterial,
        items: Vec<ApplyItem>,
        frozen: Option<Vec<ApplyDoubt>>,
    ) -> (PreparedApply, Vec<ApplyDoubt>) {
        let numbered = review.numbered();
        let mut prepared = PreparedApply {
            relations: Vec::new(),
            doubted: Vec::new(),
            rejected: Vec::new(),
            jev: None,
            warnings: Vec::new(),
        };
        let mut confirmed = Vec::new();
        for item in items {
            let reject = |reason: &str| ApplyRejection {
                item_id: item.item_id.clone(),
                reason: reason.to_string(),
            };
            let Some((pair, suggested)) = numbered.iter().find_map(|(id, finding)| match finding {
                CurateFinding::Missing {
                    pair,
                    suggested_rel,
                    ..
                } if *id == item.item_id => Some((pair, suggested_rel)),
                _ => None,
            }) else {
                prepared
                    .rejected
                    .push(reject("not a missing item of this review"));
                continue;
            };
            let (from, to) = if item.reverse {
                (pair.to.clone(), pair.from.clone())
            } else {
                (pair.from.clone(), pair.to.clone())
            };
            if material.fact(&from).map(|fact| fact.about.as_str()) != Some(about) {
                prepared.rejected.push(reject(
                    "`from` belongs to another about; apply it in a call naming that about, or reverse it",
                ));
                continue;
            }
            let Some(rel) = item.rel.clone().or_else(|| suggested.clone()) else {
                prepared
                    .rejected
                    .push(reject("no relation type: name `rel`, Jev suggested none"));
                continue;
            };
            if rel == NONE || !relation_options(pair.crosses_abouts).contains(&rel) {
                prepared.rejected.push(reject(&format!(
                    "`{rel}` cannot be declared here{}",
                    if pair.crosses_abouts {
                        "; across abouts only same_event_as and same_entity_as"
                    } else {
                        ""
                    }
                )));
                continue;
            }
            let proposal = if pair.crosses_abouts {
                let signals = match &pair.origin {
                    PairOrigin::Kernel { signals, .. } => signals
                        .iter()
                        .filter(|signal| PROPOSAL_SIGNALS.contains(&signal.as_str()))
                        .cloned()
                        .collect::<Vec<_>>(),
                    PairOrigin::Jev => Vec::new(),
                };
                if signals.is_empty() {
                    prepared.rejected.push(reject(
                        "a link across abouts needs a kernel proposal with identifier, summary or entity signals",
                    ));
                    continue;
                }
                Some(signals)
            } else {
                None
            };
            if item.confirm_doubted {
                confirmed.push(item.item_id.clone());
            }
            prepared.relations.push(PreparedRelation {
                item_id: item.item_id.clone(),
                from,
                to,
                rel,
                why: item.why,
                evidence: item.evidence,
                confidence: item.confidence,
                proposal,
                origin: pair.origin.clone(),
            });
        }

        let doubts = match frozen {
            Some(doubts) => doubts,
            None => match self.judgement {
                Some(model) if !prepared.relations.is_empty() => {
                    match model
                        .evaluate(&precheck_request(material, &prepared.relations))
                        .await
                    {
                        Ok(response) => {
                            prepared.jev = Some(JevUsage {
                                model: model.model().to_string(),
                                requests: response.requests,
                                input_tokens: response.input_tokens,
                            });
                            prepared
                                .relations
                                .iter()
                                .enumerate()
                                .filter_map(|(n, relation)| {
                                    let support = match response.answers.get(&format!("s{n}")) {
                                        Some(JudgementAnswer::Noul { yes }) => *yes,
                                        _ => return None,
                                    };
                                    let best = match response.answers.get(&format!("b{n}")) {
                                        Some(JudgementAnswer::Choice {
                                            choice,
                                            probabilities,
                                            confidence,
                                        }) => JevVerdict {
                                            choice: choice.clone(),
                                            probabilities: probabilities.clone(),
                                            confidence: *confidence,
                                        },
                                        _ => return None,
                                    };
                                    let doubted = support < DOUBT_BELOW
                                        || (best.choice != relation.rel
                                            && best.choice != NONE
                                            && best.confidence >= RETYPE_AT);
                                    doubted.then(|| ApplyDoubt {
                                        item_id: relation.item_id.clone(),
                                        support,
                                        best,
                                    })
                                })
                                .collect()
                        }
                        Err(error) => {
                            prepared.warnings.push(format!(
                                "Jev pre-write check skipped; writing without it: {error}"
                            ));
                            Vec::new()
                        }
                    }
                }
                _ => Vec::new(),
            },
        };
        let withheld = |relation: &PreparedRelation| {
            doubts.iter().any(|doubt| doubt.item_id == relation.item_id)
                && !confirmed.contains(&relation.item_id)
        };
        let (held, kept): (Vec<_>, Vec<_>) =
            prepared.relations.into_iter().partition(|r| withheld(r));
        prepared.relations = kept;
        prepared.doubted = doubts
            .iter()
            .filter(|doubt| {
                held.iter()
                    .any(|relation| relation.item_id == doubt.item_id)
            })
            .cloned()
            .collect();
        (prepared, doubts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use super::super::scripted_judgement::Scripted;
    use crate::curate::domain::candidate_pair::CandidatePair;
    use crate::curate::domain::curate_fact::CurateFact;
    use crate::curate::domain::declared_link::DeclaredLink;

    fn fact(reference: &str, about: &str) -> CurateFact {
        CurateFact {
            reference: reference.into(),
            about: about.into(),
            text: format!("text {reference}"),
        }
    }

    fn material() -> CurateMaterial {
        CurateMaterial {
            facts: vec![fact("a1", "a"), fact("a2", "a"), fact("b1", "b")],
            declared: vec![],
            pairs: vec![],
            selection: "fp".into(),
        }
    }

    fn missing(from: &str, to: &str, crosses: bool, rel: &str) -> CurateFinding {
        CurateFinding::Missing {
            pair: CandidatePair {
                from: from.into(),
                to: to.into(),
                origin: PairOrigin::Kernel {
                    signals: vec!["entity".into()],
                    why: "w".into(),
                },
                crosses_abouts: crosses,
            },
            suggested_rel: Some(rel.into()),
            verdict: None,
        }
    }

    fn review() -> CurateReview {
        CurateReview {
            findings: vec![
                missing("a1", "a2", false, "supports"),
                missing("b1", "a1", true, "same_entity_as"),
                CurateFinding::Suspect {
                    link: DeclaredLink {
                        from: "a2".into(),
                        to: "a1".into(),
                        rel: "supports".into(),
                        why: "w".into(),
                        evidence: "e".into(),
                    },
                    support: 0.1,
                    best: JevVerdict {
                        choice: "none".into(),
                        probabilities: Default::default(),
                        confidence: 0.9,
                    },
                },
            ],
            jev: None,
            warnings: vec![],
            selection: "fp".into(),
        }
    }

    fn item(id: &str) -> ApplyItem {
        ApplyItem {
            item_id: id.into(),
            why: "my why".into(),
            evidence: "my evidence".into(),
            confidence: None,
            rel: None,
            reverse: false,
            confirm_doubted: false,
        }
    }

    fn scripted(noul: f64, choice: &'static str) -> Scripted {
        Scripted {
            noul,
            choice,
            confidence: 0.9,
            calls: Mutex::new(0),
        }
    }

    #[tokio::test]
    async fn unknown_suspect_and_foreign_items_are_rejected() {
        let (prepared, _) = PrepareApply { judgement: None }
            .run(
                "a",
                &review(),
                &material(),
                vec![item("m9"), item("s0"), item("m1")],
                None,
            )
            .await;
        assert!(prepared.relations.is_empty());
        let reasons = prepared
            .rejected
            .iter()
            .map(|r| (r.item_id.as_str(), r.reason.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(reasons.len(), 3, "{reasons:?}");
        assert!(
            reasons[2].1.contains("another about"),
            "b1 owns m1: {reasons:?}"
        );
    }

    #[tokio::test]
    async fn reversing_a_crossing_pair_keeps_its_proposal() {
        let mut reversed = item("m1");
        reversed.reverse = true;
        let (prepared, _) = PrepareApply { judgement: None }
            .run("a", &review(), &material(), vec![reversed], None)
            .await;
        assert_eq!(prepared.relations.len(), 1, "{:?}", prepared.rejected);
        let relation = &prepared.relations[0];
        assert_eq!((relation.from.as_str(), relation.to.as_str()), ("a1", "b1"));
        assert_eq!(relation.rel, "same_entity_as");
        assert_eq!(relation.proposal, Some(vec!["entity".to_string()]));
    }

    #[tokio::test]
    async fn none_and_structural_types_are_rejected() {
        let mut none = item("m0");
        none.rel = Some("none".into());
        let mut structural = item("m0");
        structural.rel = Some("member_of".into());
        let (prepared, _) = PrepareApply { judgement: None }
            .run("a", &review(), &material(), vec![none, structural], None)
            .await;
        assert!(prepared.relations.is_empty());
        assert_eq!(prepared.rejected.len(), 2);
    }

    #[tokio::test]
    async fn a_doubted_item_is_withheld_until_confirmed() {
        let model = scripted(0.1, "supports");
        let (prepared, doubts) = PrepareApply {
            judgement: Some(&model),
        }
        .run("a", &review(), &material(), vec![item("m0")], None)
        .await;
        assert!(prepared.relations.is_empty());
        assert_eq!(prepared.doubted.len(), 1);
        assert_eq!(prepared.jev.as_ref().map(|u| u.requests), Some(1));

        let mut confirmed = item("m0");
        confirmed.confirm_doubted = true;
        let (again, _) = PrepareApply {
            judgement: Some(&model),
        }
        .run("a", &review(), &material(), vec![confirmed], Some(doubts))
        .await;
        assert_eq!(again.relations.len(), 1);
        assert!(again.doubted.is_empty());
        assert_eq!(
            *model.calls.lock().expect("calls"),
            1,
            "frozen doubts: no second call"
        );
    }

    #[tokio::test]
    async fn a_supported_item_passes_the_check() {
        let model = scripted(0.9, "supports");
        let (prepared, doubts) = PrepareApply {
            judgement: Some(&model),
        }
        .run("a", &review(), &material(), vec![item("m0")], None)
        .await;
        assert_eq!(prepared.relations.len(), 1);
        assert!(doubts.is_empty());
    }
}
