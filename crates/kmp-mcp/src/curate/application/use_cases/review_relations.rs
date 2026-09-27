use super::find_partners::FindPartners;
use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{pair_request, relation_options, suspect_request};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{DOUBT_BELOW, NONE, RETYPE_AT};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::partner_cap::PartnerCap;
use crate::curate::domain::partner_filter::PartnerFilter;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_failure::JudgementFailure;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Review the relations of a reading: kernel pairs, Jev partners for
/// orphans, a type for every pair, contradictions nobody declared, and
/// declared links whose reason does not hold. Writes nothing.
pub(crate) struct ReviewRelations<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
    /// The largest about whose orphans get a Jev partner choice.
    pub partner_cap: PartnerCap,
    /// What a pair of the partner round must pass before it is typed.
    pub partner_filter: PartnerFilter,
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
        let mut usage = JevUsage::new(model.model());
        let mut pairs = material.pairs.clone();
        let (partners, too_large) = FindPartners {
            cap: self.partner_cap,
            filter: self.partner_filter,
        }
        .run(model, &material, &mut usage, &mut review.warnings)
        .await;
        pairs.extend(partners);
        let mut ask = async |request: JudgementRequest| -> Option<JudgementResponse> {
            match model.evaluate(&request).await {
                Ok(response) => {
                    usage.add(&response);
                    Some(response)
                }
                Err(error) => {
                    review
                        .warnings
                        .push(JudgementFailure::warning(&error, "using kernel pairs only"));
                    None
                }
            }
        };

        let typed = if pairs.is_empty() {
            Some(JudgementResponse::empty(model.model()))
        } else {
            ask(pair_request(&material, &pairs)).await
        };
        let Some(typed) = typed else {
            review.findings = untyped(&material.pairs, max_pairs);
            review.warnings.extend(too_large);
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
        review.warnings.extend(too_large);
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

#[cfg(test)]
#[path = "review_relations_tests.rs"]
mod tests;
