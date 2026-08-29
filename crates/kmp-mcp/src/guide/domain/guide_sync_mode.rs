#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideSyncMode {
    Apply,
    DryRun,
}

impl GuideSyncMode {
    pub fn should_apply(self) -> bool {
        self == Self::Apply
    }
}
