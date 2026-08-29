use crate::application::dto::evidence_interpretation_input::EvidenceInterpretationInput;
use crate::application::dto::evidence_interpretation_output::EvidenceInterpretationOutput;
use crate::domain::interpretation_error::InterpretationError;

/// Outbound port implemented by a deterministic evidence interpreter.
pub trait EvidenceValuePlugin: Send + Sync {
    fn id(&self) -> &'static str;

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError>;
}
