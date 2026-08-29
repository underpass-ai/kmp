//! Stable hexagonal boundary for KMP evidence interpretation plugins.
//!
//! Domain value objects carry invariants, application DTOs define the wire
//! contract, and ports define behavior supplied by plugin adapters. Storage,
//! traversal and runtime infrastructure remain outside this crate.

pub mod application;
pub mod domain;
pub mod ports;

pub use application::dto::derivation_operand::DerivationOperand;
pub use application::dto::derivation_request::DerivationRequest;
pub use application::dto::derivation_result::DerivationResult;
pub use application::dto::evidence_fragment::EvidenceFragment;
pub use application::dto::evidence_interpretation_input::EvidenceInterpretationInput;
pub use application::dto::evidence_interpretation_output::EvidenceInterpretationOutput;
pub use application::dto::interpreted_value_mention::InterpretedValueMention;
pub use domain::calendar_date::CalendarDate;
pub use domain::currency_code::CurrencyCode;
pub use domain::derivation_operation::DerivationOperation;
pub use domain::evidence_segment_kind::EvidenceSegmentKind;
pub use domain::interpretation_error::InterpretationError;
pub use domain::interpreted_value::InterpretedValue;
pub use domain::math_expression_notation::MathExpressionNotation;
pub use domain::operand_label::OperandLabel;
pub use domain::operand_role::OperandRole;
pub use domain::source_code_segment_kind::SourceCodeSegmentKind;
pub use domain::text_span::TextSpan;
pub use ports::evidence_derivation_plugin::EvidenceDerivationPlugin;
pub use ports::evidence_value_plugin::EvidenceValuePlugin;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_builders_preserve_the_existing_contract() {
        let operand = DerivationOperand::included(
            "turn:42",
            InterpretedValue::number(3.0, Some("items".to_string())),
        )
        .with_role(OperandRole::CountedItem)
        .with_entity("payment-service");

        assert_eq!(CurrencyCode::new(" usd ").expect("code").as_str(), "USD");
        assert_eq!(operand.ref_id, "turn:42");
        assert_eq!(operand.label, OperandLabel::Include);
        assert_eq!(operand.role, Some(OperandRole::CountedItem));
        assert_eq!(operand.entity.as_deref(), Some("payment-service"));
    }
}
