/// Typed lifecycle disposition for one owned component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvergenceStatus {
    PlannedChange,
    Changed,
    Unchanged,
    /// Detected but deliberately left alone; the convergence says why.
    Skipped,
}
