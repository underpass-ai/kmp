/// How a tool call failed, for metrics that must not read messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolErrorKind {
    Backend,
    Validation,
}

impl ToolErrorKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Backend => "backend",
            Self::Validation => "validation",
        }
    }
}
