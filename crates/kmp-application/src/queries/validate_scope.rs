use std::collections::BTreeSet;

use crate::queries::QueryApplicationService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeValidation {
    pub allowed: bool,
    pub required_scopes: Vec<String>,
    pub provided_scopes: Vec<String>,
    pub missing_scopes: Vec<String>,
    pub extra_scopes: Vec<String>,
    pub reason: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidateScopeQuery {
    pub required_scopes: Vec<String>,
    pub provided_scopes: Vec<String>,
}

#[derive(Debug)]
pub struct ValidateScopeUseCase;

impl ValidateScopeUseCase {
    pub fn execute(required_scopes: &[String], provided_scopes: &[String]) -> ScopeValidation {
        let required = dedupe_scopes(required_scopes);
        let provided = dedupe_scopes(provided_scopes);

        let required_set: BTreeSet<_> = required.iter().cloned().collect();
        let provided_set: BTreeSet<_> = provided.iter().cloned().collect();

        let missing_scopes = required_set
            .difference(&provided_set)
            .cloned()
            .collect::<Vec<_>>();
        let extra_scopes = provided_set
            .difference(&required_set)
            .cloned()
            .collect::<Vec<_>>();
        let allowed = missing_scopes.is_empty() && extra_scopes.is_empty();
        let reason = if allowed {
            "scope validation passed".to_string()
        } else {
            "scope validation failed".to_string()
        };

        ScopeValidation {
            allowed,
            required_scopes: required,
            provided_scopes: provided,
            missing_scopes,
            extra_scopes,
            reason,
            diagnostics: Vec::new(),
        }
    }
}

impl<G, D, S> QueryApplicationService<G, D, S> {
    pub fn validate_scope(&self, query: ValidateScopeQuery) -> ScopeValidation {
        ValidateScopeUseCase::execute(&query.required_scopes, &query.provided_scopes)
    }
}

pub fn dedupe_scopes(scopes: &[String]) -> Vec<String> {
    scopes
        .iter()
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
