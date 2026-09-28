/// What one optional file beside a store did when the store was read, as an
/// operator-facing report shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StoreConfigState {
    /// Not beside the store, or turned off where it can be named elsewhere.
    Absent,
    On,
    /// On, but not as written: the reason says what was read in its place.
    OnWithWarning(String),
    /// Present and not applied, with the reason.
    Rejected(String),
}
