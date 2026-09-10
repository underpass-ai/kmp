#[derive(Debug, Clone, Copy)]
pub(crate) enum UseOutcome {
    Returned,
    Rejected,
    Unknown,
}
