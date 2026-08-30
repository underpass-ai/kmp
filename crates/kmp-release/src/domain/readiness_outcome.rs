/// What one release readiness check concluded. A failed outcome carries the
/// reason so the whole report can be rendered at once instead of one release
/// attempt at a time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadinessOutcome {
    Passed(String),
    Failed(String),
}

impl ReadinessOutcome {
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Passed(detail) | Self::Failed(detail) => detail,
        }
    }
}
