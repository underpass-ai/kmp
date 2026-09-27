//! One record of a `memories` packet, as the batch compiler hands it on.

use serde_json::Value;

/// A packet record with its position and the one field, if any, whose
/// refusal was already reported while the packet was read, so the compiler's
/// own finding about that field is not reported a second time.
#[derive(Clone, Copy)]
pub(super) struct Member<'a> {
    pub(super) index: usize,
    pub(super) memory: &'a Value,
    pub(super) superseded: Option<&'static str>,
}
