/// Why a store lease could not be claimed.
#[derive(Debug)]
pub(crate) enum LeaseClaimError {
    /// Another claim holds the store in a mode that excludes this one.
    Busy,
    /// The lease file could not be opened or locked.
    Io(String),
}
