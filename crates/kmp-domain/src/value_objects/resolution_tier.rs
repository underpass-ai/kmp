use crate::DomainError;
use crate::value_objects::KmpMode;

/// Resolution tier for multi-resolution bundle rendering.
///
/// Bundles are assembled in three tiers of decreasing criticality:
/// - L0: compact summary (~100 tokens) — always fits
/// - L1: causal spine (~500 tokens) — root + focus + causal chain
/// - L2: evidence pack — remaining budget fills with details and structural data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResolutionTier {
    L0Summary,
    L1CausalSpine,
    L2EvidencePack,
}

impl ResolutionTier {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value.trim() {
            "l0_summary" => Ok(Self::L0Summary),
            "l1_causal_spine" => Ok(Self::L1CausalSpine),
            "l2_evidence_pack" => Ok(Self::L2EvidencePack),
            other => Err(DomainError::InvalidState(format!(
                "invalid resolution tier `{other}`"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::L0Summary => "l0_summary",
            Self::L1CausalSpine => "l1_causal_spine",
            Self::L2EvidencePack => "l2_evidence_pack",
        }
    }

    /// All tiers in rendering order.
    pub fn all() -> &'static [Self] {
        &[Self::L0Summary, Self::L1CausalSpine, Self::L2EvidencePack]
    }
}

/// Per-tier token budget allocation.
///
/// L0 and L1 get fixed ceilings; L2 gets the remainder.
/// If the total budget is smaller than L0+L1 ceilings, tiers
/// are filled in order until the budget is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierBudget {
    pub l0: u32,
    pub l1: u32,
    pub l2: u32,
}

const L0_CEILING: u32 = 100;
const L1_CEILING: u32 = 500;

impl TierBudget {
    /// Distribute a total token budget across tiers.
    ///
    /// L0 gets up to 100 tokens, L1 up to 500, L2 gets the rest.
    pub fn from_total(total: u32) -> Self {
        let l0 = total.min(L0_CEILING);
        let remaining = total.saturating_sub(l0);
        let l1 = remaining.min(L1_CEILING);
        let l2 = remaining.saturating_sub(l1);
        Self { l0, l1, l2 }
    }

    /// Unlimited budget — no tier is constrained.
    pub fn unlimited() -> Self {
        Self {
            l0: u32::MAX,
            l1: u32::MAX,
            l2: u32::MAX,
        }
    }

    /// Resume-focused budget: L0 gets ceiling, L1 gets ALL remaining, L2 gets 0.
    ///
    /// This lets the causal spine use the full token budget minus the compact summary,
    /// dropping all evidence/structural content to maximize causal chain preservation.
    pub fn from_total_resume_focused(total: u32) -> Self {
        let l0 = total.min(L0_CEILING);
        let l1 = total.saturating_sub(l0);
        Self { l0, l1, l2: 0 }
    }

    /// Distribute budget using mode-specific strategy.
    pub fn from_total_with_mode(total: u32, mode: KmpMode) -> Self {
        match mode {
            super::KmpMode::ResumeFocused => Self::from_total_resume_focused(total),
            _ => Self::from_total(total),
        }
    }

    pub fn total(&self) -> u32 {
        self.l0.saturating_add(self.l1).saturating_add(self.l2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for tier in ResolutionTier::all() {
            let parsed = ResolutionTier::parse(tier.as_str()).expect("valid tier");
            assert_eq!(&parsed, tier);
        }
    }

    #[test]
    fn parse_invalid_returns_error() {
        assert!(ResolutionTier::parse("invalid").is_err());
    }

    #[test]
    fn ordering_matches_tier_priority() {
        assert!(ResolutionTier::L0Summary < ResolutionTier::L1CausalSpine);
        assert!(ResolutionTier::L1CausalSpine < ResolutionTier::L2EvidencePack);
    }

    #[test]
    fn budget_from_total_distributes_correctly() {
        let b = TierBudget::from_total(4096);
        assert_eq!(b.l0, 100);
        assert_eq!(b.l1, 500);
        assert_eq!(b.l2, 3496);
        assert_eq!(b.total(), 4096);
    }

    #[test]
    fn budget_small_total_fills_l0_first() {
        let b = TierBudget::from_total(50);
        assert_eq!(b.l0, 50);
        assert_eq!(b.l1, 0);
        assert_eq!(b.l2, 0);
    }

    #[test]
    fn budget_medium_total_fills_l0_and_partial_l1() {
        let b = TierBudget::from_total(300);
        assert_eq!(b.l0, 100);
        assert_eq!(b.l1, 200);
        assert_eq!(b.l2, 0);
    }

    #[test]
    fn budget_zero() {
        let b = TierBudget::from_total(0);
        assert_eq!(b.l0, 0);
        assert_eq!(b.l1, 0);
        assert_eq!(b.l2, 0);
    }

    #[test]
    fn unlimited_budget() {
        let b = TierBudget::unlimited();
        assert_eq!(b.l0, u32::MAX);
        assert!(b.total() > 0);
    }

    #[test]
    fn budget_resume_focused_gives_all_to_l1() {
        let b = TierBudget::from_total_resume_focused(512);
        assert_eq!(b.l0, 100);
        assert_eq!(b.l1, 412);
        assert_eq!(b.l2, 0);
        assert_eq!(b.total(), 512);
    }

    #[test]
    fn budget_with_mode_dispatches_correctly() {
        let resume = TierBudget::from_total_with_mode(4096, KmpMode::ResumeFocused);
        assert_eq!(resume.l2, 0, "resume_focused should give nothing to L2");

        let default = TierBudget::from_total_with_mode(4096, KmpMode::ReasonPreserving);
        assert!(default.l2 > 0, "reason_preserving should have L2 budget");
    }
}
