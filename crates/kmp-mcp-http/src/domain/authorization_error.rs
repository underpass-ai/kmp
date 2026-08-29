#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationError {
    pub reason: String,
    pub required_scope: Option<&'static str>,
}

impl AuthorizationError {
    pub(crate) fn missing_scope(scope: &'static str) -> Self {
        Self {
            reason: format!("required scope `{scope}` is missing"),
            required_scope: Some(scope),
        }
    }

    pub(crate) fn denied(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            required_scope: None,
        }
    }
}
