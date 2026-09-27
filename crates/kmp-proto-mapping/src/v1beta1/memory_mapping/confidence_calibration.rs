use std::sync::OnceLock;

use kmp_proto::v1beta1::MemoryConfidence;
use serde::Deserialize;

use super::confidence_rule::ConfidenceRule;
use super::confidence_traits::ConfidenceTraits;

const SOURCE: &str = include_str!("../../../language/confidence_calibration.json");

/// A calibration table for `proof.confidence` (P16, after Trust or Escalate).
///
/// Versioned data, not code: `language/confidence_calibration.json` ships the
/// rules and, beside them, the measurement they came from (`measured`, read
/// by people, ignored here). A rule only ever moves `high` down to `medium`;
/// it never raises a confidence, and never touches `medium` or below. `high`
/// is what the bench certifies with a one-sided Clopper-Pearson bound, so the
/// table is how an answer that the measurement could not stand behind stops
/// claiming it.
///
/// Off unless a store asks for it in `ask-gate.json`
/// (`"confidence_calibration": "shipped"`, or an inline table).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceCalibration {
    version: String,
    #[serde(default)]
    measured: serde_json::Value,
    demote_high: Vec<ConfidenceRule>,
}

impl ConfidenceCalibration {
    /// The table this kernel ships.
    pub fn shipped() -> &'static Self {
        static SHIPPED: OnceLock<ConfidenceCalibration> = OnceLock::new();
        SHIPPED.get_or_init(|| Self::parse(SOURCE).expect("the shipped calibration parses"))
    }

    /// A table from its JSON text; refuses unknown fields and a missing
    /// version, so a typo is an error and not a silent no-op.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: Self = serde_json::from_str(text)
            .map_err(|error| format!("not a confidence calibration: {error}"))?;
        if table.version.trim().is_empty() {
            return Err("a confidence calibration names its version".to_string());
        }
        Ok(table)
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// The measurement the table records beside its rules, for people.
    pub fn measured(&self) -> &serde_json::Value {
        &self.measured
    }

    /// The confidence stated after the table: `high` becomes `medium` when a
    /// rule matches; anything else passes through untouched.
    pub(super) fn apply(
        &self,
        confidence: MemoryConfidence,
        traits: &ConfidenceTraits,
    ) -> MemoryConfidence {
        if confidence == MemoryConfidence::High
            && self.demote_high.iter().any(|rule| rule.matches(traits))
        {
            MemoryConfidence::Medium
        } else {
            confidence
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::concept_coverage::ConceptCoverage;
    use super::super::confidence_branch::ConfidenceBranch;
    use super::*;

    fn traits(negated_anchor: bool) -> ConfidenceTraits {
        ConfidenceTraits {
            branch: ConfidenceBranch::Anchored,
            coverage: ConceptCoverage {
                matched: 4,
                asked: 4,
            },
            negated_anchor,
            cited: 3,
            enumerative: false,
        }
    }

    const TABLE: &str =
        r#"{"version":"t1","demote_high":[{"id":"negated","negated_anchor":true}]}"#;

    #[test]
    fn a_matching_rule_demotes_high_and_only_high() {
        let table = ConfidenceCalibration::parse(TABLE).expect("parses");
        assert_eq!(
            table.apply(MemoryConfidence::High, &traits(true)),
            MemoryConfidence::Medium
        );
        assert_eq!(
            table.apply(MemoryConfidence::High, &traits(false)),
            MemoryConfidence::High
        );
        for level in [
            MemoryConfidence::Medium,
            MemoryConfidence::Low,
            MemoryConfidence::Unknown,
        ] {
            assert_eq!(table.apply(level, &traits(true)), level);
        }
    }

    #[test]
    fn the_shipped_table_parses_and_is_versioned() {
        let shipped = ConfidenceCalibration::shipped();
        assert!(shipped.version().starts_with("confidence-calibration."));
    }

    #[test]
    fn malformed_tables_are_refused() {
        assert!(ConfidenceCalibration::parse("{").is_err());
        assert!(ConfidenceCalibration::parse(r#"{"version":"","demote_high":[]}"#).is_err());
        assert!(
            ConfidenceCalibration::parse(r#"{"version":"x","demote_high":[],"extra":1}"#).is_err()
        );
        assert!(ConfidenceCalibration::parse(r#"{"version":"x"}"#).is_err());
    }
}
