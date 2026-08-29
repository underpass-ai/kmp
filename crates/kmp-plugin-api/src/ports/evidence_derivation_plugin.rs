use crate::application::dto::derivation_request::DerivationRequest;
use crate::application::dto::derivation_result::DerivationResult;
use crate::domain::interpretation_error::InterpretationError;

/// Outbound port implemented by a deterministic evidence derivation.
pub trait EvidenceDerivationPlugin: Send + Sync {
    fn id(&self) -> &'static str;

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError>;
}
