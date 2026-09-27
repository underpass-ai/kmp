//! What a recorder needs to say where a call came from.

use super::call_origin::CallOrigin;
use super::fingerprint_salt::FingerprintSalt;

/// Where a call came from and the salt its fingerprints are keyed with.
/// Recorders without one log what they always logged.
#[derive(Clone, Copy)]
pub(crate) struct CallSource<'a> {
    pub(crate) origin: &'a CallOrigin,
    pub(crate) salt: Option<&'a FingerprintSalt>,
}
