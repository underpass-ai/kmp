//! Read continuations: a page hands back a short handle instead of restating
//! the call it continues (#544 C3). The handle names a retained call; it
//! grants nothing, and every transport authorizes the resolved request.
//! Bounded, expiring transport state beside the store, never memory or
//! evidence.
mod adapters;
mod continuation_error;
mod ports;
mod read_continuation;
mod read_continuation_id;

pub(crate) use adapters::SqliteContinuationStore;
pub(crate) use continuation_error::ContinuationError;
pub(crate) use ports::ContinuationStore;
pub(crate) use read_continuation::ReadContinuation;
pub(crate) use read_continuation_id::ReadContinuationId;
