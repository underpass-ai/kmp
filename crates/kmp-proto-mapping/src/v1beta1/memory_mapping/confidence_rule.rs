use serde::Deserialize;

use super::confidence_branch::ConfidenceBranch;
use super::confidence_traits::ConfidenceTraits;

/// One demotion of a calibration table: when every condition it states
/// holds, a `high` answer is stated as `medium`. An absent condition does
/// not constrain; a rule with no condition matches nothing, so an empty
/// entry cannot demote everything by accident.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfidenceRule {
    pub(super) id: String,
    #[serde(default)]
    branch: Option<ConfidenceBranch>,
    #[serde(default)]
    negated_anchor: Option<bool>,
    #[serde(default)]
    enumerative: Option<bool>,
    /// The best memory is credited with fewer question concepts than this.
    #[serde(default)]
    matched_concepts_below: Option<usize>,
    /// The best memory leaves out more question concepts than this.
    #[serde(default)]
    missed_concepts_above: Option<usize>,
    /// The core retained fewer citations than this.
    #[serde(default)]
    cited_below: Option<usize>,
}

impl ConfidenceRule {
    pub(super) fn matches(&self, traits: &ConfidenceTraits) -> bool {
        let conditions = [
            self.branch.map(|branch| traits.branch == branch),
            self.negated_anchor.map(|on| traits.negated_anchor == on),
            self.enumerative.map(|on| traits.enumerative == on),
            self.matched_concepts_below
                .map(|floor| traits.coverage.matched < floor),
            self.missed_concepts_above
                .map(|ceiling| traits.coverage.missed() > ceiling),
            self.cited_below.map(|floor| traits.cited < floor),
        ];
        conditions.iter().any(Option::is_some) && conditions.iter().flatten().all(|held| *held)
    }
}

#[cfg(test)]
mod tests {
    use super::super::concept_coverage::ConceptCoverage;
    use super::*;

    fn traits(branch: ConfidenceBranch, matched: usize, asked: usize) -> ConfidenceTraits {
        ConfidenceTraits {
            branch,
            coverage: ConceptCoverage { matched, asked },
            negated_anchor: false,
            cited: 1,
            enumerative: false,
        }
    }

    fn rule(json: &str) -> ConfidenceRule {
        serde_json::from_str(json).expect("rule parses")
    }

    #[test]
    fn every_stated_condition_must_hold() {
        let thin = rule(r#"{"id":"thin","branch":"unanchored","matched_concepts_below":3}"#);
        assert!(thin.matches(&traits(ConfidenceBranch::Unanchored, 2, 3)));
        assert!(!thin.matches(&traits(ConfidenceBranch::Unanchored, 3, 4)));
        assert!(!thin.matches(&traits(ConfidenceBranch::Anchored, 2, 3)));
        let missed = rule(r#"{"id":"missed","missed_concepts_above":1}"#);
        assert!(missed.matches(&traits(ConfidenceBranch::Anchored, 3, 5)));
        assert!(!missed.matches(&traits(ConfidenceBranch::Anchored, 4, 5)));
        let single = rule(r#"{"id":"single","cited_below":2,"enumerative":true}"#);
        let mut enumerative = traits(ConfidenceBranch::Anchored, 4, 4);
        enumerative.enumerative = true;
        assert!(single.matches(&enumerative));
        enumerative.cited = 2;
        assert!(!single.matches(&enumerative));
    }

    #[test]
    fn a_rule_without_conditions_matches_nothing() {
        assert!(!rule(r#"{"id":"empty"}"#).matches(&traits(ConfidenceBranch::Anchored, 0, 9)));
    }

    #[test]
    fn a_negated_anchor_rule_reads_the_flag() {
        let negated = rule(r#"{"id":"negated","negated_anchor":true}"#);
        let mut item = traits(ConfidenceBranch::Anchored, 5, 5);
        assert!(!negated.matches(&item));
        item.negated_anchor = true;
        assert!(negated.matches(&item));
    }

    #[test]
    fn unknown_conditions_are_refused() {
        assert!(serde_json::from_str::<ConfidenceRule>(r#"{"id":"x","weight":0.3}"#).is_err());
    }
}
