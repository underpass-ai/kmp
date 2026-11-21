use super::find_partners::FindPartners;
use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{
    backward_option, forward_option, pair_request, relation_options, suspect_request,
};
use crate::curate::application::pair_reading::{
    chosen_probability, reading_of, typed_pair, verdict_of,
};
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
use crate::serving::noul_reading::NoulReading;
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
            if let Some(verdict) = typed_pair(&typed, n) {
                missing.push(CurateFinding::Missing {
                    pair,
                    suggested_rel: Some(verdict.choice.clone()),
                    verdict: Some(verdict),
                });
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
                let direction = direction_of(&audit, n, &link.rel);
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

/// How strongly the judge reads item `n`'s relation the declared way round
/// (`d<n>`): the declared option over both. None when its companion (`e<n>`,
/// does either way hold?) reads no, which is a matter for support, not
/// direction, or when it was not asked or not answered.
pub(crate) fn direction_of(response: &JudgementResponse, n: usize, rel: &str) -> Option<f64> {
    if reading_of(response, &format!("e{n}"))? == NoulReading::No {
        return None;
    }
    let JudgementAnswer::Choice { probabilities, .. } = response.answers.get(&format!("d{n}"))?
    else {
        return None;
    };
    let forward = probabilities
        .get(&forward_option(rel))
        .copied()
        .unwrap_or(0.0);
    let backward = probabilities
        .get(&backward_option(rel))
        .copied()
        .unwrap_or(0.0);
    (forward + backward > 0.0).then(|| forward / (forward + backward))
}

/// Why a declaration, or an item about to be written, is doubted: its reason
/// reads no (support at or below the line), Jev would type it otherwise (the
/// probability of another offered type at or above the retype line, never
/// its `confidence`), or it runs the wrong way.
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
    // `none` is no longer offered; a verdict that names it is never a type.
    if offered && best.choice != rel && best.choice != NONE && chosen_probability(best) >= RETYPE_AT
    {
        reasons.push("type");
    }
    if direction.is_some_and(|direction| direction < DOUBT_BELOW) {
        reasons.push("direction");
    }
    reasons
}

/// What orders the missing relations: the chosen type's probability, never
/// `confidence`, which rescales it by how many types were offered (a pair
/// across abouts is offered two).
fn confidence(finding: &CurateFinding) -> f64 {
    match finding {
        CurateFinding::Missing {
            verdict: Some(verdict),
            ..
        } => chosen_probability(verdict),
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
