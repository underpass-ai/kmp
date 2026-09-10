#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum GuidancePurpose {
    #[default]
    Continue,
    Audit,
    History,
    Answer,
}

impl GuidancePurpose {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Audit => "audit",
            Self::History => "history",
            Self::Answer => "answer",
        }
    }
}
