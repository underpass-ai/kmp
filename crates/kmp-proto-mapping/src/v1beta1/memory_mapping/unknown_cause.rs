use kmp_proto::v1beta1::UnknownReason;

/// Why a gated ask came back UNKNOWN, always one of the contract's reasons.
///
/// Under the gate every UNKNOWN names its reason, whichever branch decided
/// it: a span that left a bearing match outside it is `out_of_window`; the
/// anchored gate's own reason stands next; a question without a required
/// anchor, or one the gate answered but whose citations `max_entries` or the
/// evidence cap left out, falls back on what retrieval found —
/// `no_candidates` when nothing was retained, `no_bearing` when something
/// was and none of it answers.
pub(super) fn unknown_cause(
    nearest_outside: bool,
    gate_reason: Option<UnknownReason>,
    evidence_retained: usize,
) -> UnknownReason {
    if nearest_outside {
        return UnknownReason::OutOfWindow;
    }
    match gate_reason {
        Some(reason) if reason != UnknownReason::Unspecified => reason,
        _ if evidence_retained == 0 => UnknownReason::NoCandidates,
        _ => UnknownReason::NoBearing,
    }
}
