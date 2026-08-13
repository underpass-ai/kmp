use kmp_domain::{KmpMode, ResolutionTier, TierBudget, TokenEstimator};

use super::bundle_section_renderer::TaggedSection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationMetadata {
    pub budget_requested: u32,
    pub budget_used: u32,
    pub total_before_truncation: u32,
    pub sections_kept: u32,
    pub sections_dropped: u32,
    pub token_estimator: String,
}

/// Tier-aware truncation: L0 guaranteed, L1 prioritized, L2 sacrificed.
///
/// Unlike greedy sequential packing, this continues past L2 sections that
/// don't fit — later L1 sections can still be included if their tier budget
/// allows. Returns `(content, source_id)` pairs and truncation metadata.
pub(crate) fn limit_sections_by_tier_budget(
    sections: Vec<TaggedSection>,
    token_budget: Option<u32>,
    resolved_mode: KmpMode,
    estimator: &dyn TokenEstimator,
) -> (Vec<(String, String)>, Option<TruncationMetadata>) {
    let Some(budget) = token_budget else {
        let pairs = sections
            .into_iter()
            .map(|s| (s.content, s.source_id))
            .collect();
        return (pairs, None);
    };

    let tier_budget = TierBudget::from_total_with_mode(budget, resolved_mode);
    let total_sections = sections.len() as u32;
    let total_before: u32 = sections
        .iter()
        .map(|s| estimator.estimate_tokens(&s.content))
        .sum();

    let mut l0_used = 0u32;
    let mut l1_used = 0u32;
    let mut l2_used = 0u32;
    let mut kept = Vec::new();
    let mut total_used = 0u32;

    for section in sections {
        let tokens = estimator.estimate_tokens(&section.content);
        let (tier_used, tier_cap) = match section.tier {
            ResolutionTier::L0Summary => (&mut l0_used, tier_budget.l0),
            ResolutionTier::L1CausalSpine => (&mut l1_used, tier_budget.l1),
            ResolutionTier::L2EvidencePack => (&mut l2_used, tier_budget.l2),
        };

        // First section always included (L0 anchor). Otherwise check tier + total budget.
        let fits_tier = kept.is_empty() || *tier_used + tokens <= tier_cap;
        let fits_total = kept.is_empty() || total_used + tokens <= budget;

        if fits_tier && fits_total {
            *tier_used += tokens;
            total_used += tokens;
            kept.push((section.content, section.source_id));
        }
        // Don't break — a later section from a different tier may still fit.
    }

    let sections_kept = kept.len() as u32;
    let truncation = TruncationMetadata {
        budget_requested: budget,
        budget_used: total_used,
        total_before_truncation: total_before,
        sections_kept,
        sections_dropped: total_sections - sections_kept,
        token_estimator: estimator.name().to_string(),
    };

    (kept, Some(truncation))
}
