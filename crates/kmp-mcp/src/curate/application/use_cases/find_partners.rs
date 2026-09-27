use kmp_proto_mapping::v1beta1::PartnerShortlist;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::confirm_request;
use crate::curate::application::partner_plan::PartnerPlan;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::curate::domain::partner_cap::PartnerCap;
use crate::curate::domain::partner_filter::PartnerFilter;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_failure::JudgementFailure;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// The partner round of a review without `focus`: for every about with
/// orphans and at most `cap` current facts, the pairs Jev proposes for
/// them ([`PartnerPlan`]), then the store's [`PartnerFilter`]. Abouts past
/// the cap are named instead of skipped in silence.
pub(crate) struct FindPartners {
    pub cap: PartnerCap,
    pub filter: PartnerFilter,
}

impl FindPartners {
    /// Jev's pairs, and one warning per about too large for the round. Each
    /// judgement's cost is added to `usage`; a failed one leaves its warning
    /// in `warnings` and stops the round with what it had.
    pub(crate) async fn run(
        &self,
        model: &dyn JudgementModel,
        material: &CurateMaterial,
        usage: &mut JevUsage,
        warnings: &mut Vec<String>,
    ) -> (Vec<CandidatePair>, Vec<String>) {
        let mut ask = Asker {
            model,
            usage,
            warnings,
        };
        let (mut found, mut too_large) = (Vec::new(), Vec::new());
        let mut abouts = material
            .orphans()
            .iter()
            .map(|fact| fact.about.clone())
            .collect::<Vec<_>>();
        abouts.sort();
        abouts.dedup();
        'abouts: for about in abouts {
            let facts = material
                .facts
                .iter()
                .filter(|fact| fact.about == about)
                .collect::<Vec<_>>();
            // Past the cap the review says so instead of going quiet, and
            // names the way that scales: a focused review shortlists each
            // fact's partners.
            if !self.cap.admits(facts.len()) {
                too_large.push(format!(
                    "`{about}` has {} current facts: Jev looks for orphans' partners only in abouts of at most {}; review with `focus` on the facts to pair, which reads each one's shortlisted partners",
                    facts.len(),
                    self.cap.facts()
                ));
            }
            let orphans = material
                .orphans()
                .into_iter()
                .filter(|fact| fact.about == about)
                .collect::<Vec<_>>();
            let Some(plan) = PartnerPlan::new(&facts, &orphans, self.cap) else {
                continue;
            };
            let mut first = Vec::new();
            for request in plan.first_requests() {
                let Some(response) = ask.ask(request).await else {
                    break 'abouts;
                };
                first.push(response.answers);
            }
            let (mut chosen, last) = plan.after_first(&first);
            if let Some(request) = last {
                let Some(response) = ask.ask(request).await else {
                    break 'abouts;
                };
                chosen.extend(plan.after_final(&response.answers));
            }
            // In the order of the orphans' keys, as one choice always
            // answered them.
            chosen.sort();
            let mut pairs = chosen
                .iter()
                .filter_map(|(own, partner)| {
                    Some(CandidatePair {
                        from: plan.reference(own)?.to_string(),
                        to: plan.reference(partner)?.to_string(),
                        origin: PairOrigin::Jev,
                        crosses_abouts: false,
                    })
                })
                .collect::<Vec<_>>();
            if self.filter == PartnerFilter::RareTerm {
                let shortlist = PartnerShortlist::over(
                    facts
                        .iter()
                        .map(|fact| (fact.reference.as_str(), fact.text.as_str())),
                );
                let rare = PartnerFilter::rare_within(facts.len());
                pairs.retain(|pair| {
                    shortlist
                        .rarest_shared_term(&pair.from, &pair.to)
                        .is_some_and(|carriers| carriers <= rare)
                });
            }
            found.extend(pairs);
        }
        if self.filter == PartnerFilter::Confirm && !found.is_empty() {
            let Some(confirmed) = ask.ask(confirm_request(material, &found)).await else {
                return (Vec::new(), too_large);
            };
            let mut n = 0;
            found.retain(|_| {
                let key = format!("c{n}");
                n += 1;
                matches!(
                    confirmed.answers.get(&key),
                    Some(JudgementAnswer::Noul { yes }) if *yes >= PartnerFilter::CONFIRM_AT
                )
            });
        }
        (found, too_large)
    }
}

/// One judgement of the round: its cost counted, its failure warned.
struct Asker<'a, 'b> {
    model: &'a dyn JudgementModel,
    usage: &'b mut JevUsage,
    warnings: &'b mut Vec<String>,
}

impl Asker<'_, '_> {
    async fn ask(&mut self, request: JudgementRequest) -> Option<JudgementResponse> {
        match self.model.evaluate(&request).await {
            Ok(response) => {
                self.usage.add(&response);
                Some(response)
            }
            Err(error) => {
                self.warnings
                    .push(JudgementFailure::warning(&error, "using kernel pairs only"));
                None
            }
        }
    }
}

#[cfg(test)]
#[path = "find_partners_tests.rs"]
mod tests;
