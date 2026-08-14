use kmp_application::ScopeValidation;
use kmp_proto::v1beta1::ScopeValidationResult;

pub fn proto_scope_validation_v1beta1(result: &ScopeValidation) -> ScopeValidationResult {
    ScopeValidationResult {
        allowed: result.allowed,
        required_scopes: result.required_scopes.clone(),
        provided_scopes: result.provided_scopes.clone(),
        missing_scopes: result.missing_scopes.clone(),
        extra_scopes: result.extra_scopes.clone(),
        reason: result.reason.clone(),
        diagnostics: result.diagnostics.clone(),
    }
}
