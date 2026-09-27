/// What one pass of the sidecar's maintenance did, for telemetry and tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CatchUpReport {
    /// The log position the sidecar stands at afterwards.
    pub(crate) position: u64,
    /// Events read after the position the sidecar stood at.
    pub(crate) events: u64,
    pub(crate) abouts_refreshed: u64,
    pub(crate) abouts_rebuilt: u64,
    /// Candidate rows recomputed (changed or confirmed).
    pub(crate) rows: u64,
    /// The sidecar was of another derivation or log and started again.
    pub(crate) reset: bool,
    /// False when another process moved the sidecar first; nothing written.
    pub(crate) committed: bool,
    pub(crate) elapsed_us: u64,
    /// The about the ask asked for was not built: it has fewer entries
    /// than the store indexes (`min_about_entries`).
    pub(crate) below_threshold: bool,
}
