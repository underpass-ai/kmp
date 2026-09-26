use kmp_proto::v1beta1::UnknownReason;

use super::unknown_cause::unknown_cause;

#[test]
fn a_bearing_match_outside_the_span_is_out_of_window_whatever_the_gate_said() {
    assert_eq!(
        unknown_cause(true, Some(UnknownReason::AttributeNotFound), 3),
        UnknownReason::OutOfWindow
    );
    assert_eq!(unknown_cause(true, None, 0), UnknownReason::OutOfWindow);
}

#[test]
fn the_gate_reason_stands_when_it_named_one() {
    for reason in [
        UnknownReason::AnchorAbsentInSelection,
        UnknownReason::AttributeNotFound,
        UnknownReason::NoBearing,
    ] {
        assert_eq!(unknown_cause(false, Some(reason), 4), reason);
    }
}

#[test]
fn a_question_without_anchors_says_whether_anything_was_retained() {
    assert_eq!(unknown_cause(false, None, 0), UnknownReason::NoCandidates);
    assert_eq!(unknown_cause(false, None, 2), UnknownReason::NoBearing);
}

#[test]
fn an_answered_verdict_whose_citations_were_capped_away_still_names_a_reason() {
    // The gate answered (reason unspecified), but no citation survived the
    // cap: the response is UNKNOWN and must not leave the reason unset.
    assert_eq!(
        unknown_cause(false, Some(UnknownReason::Unspecified), 0),
        UnknownReason::NoCandidates
    );
    assert_eq!(
        unknown_cause(false, Some(UnknownReason::Unspecified), 5),
        UnknownReason::NoBearing
    );
}
