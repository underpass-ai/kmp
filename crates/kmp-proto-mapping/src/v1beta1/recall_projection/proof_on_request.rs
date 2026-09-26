//! An answered ask keeps the rest of its proof on request.
//!
//! The anchored gate settles an ask as `answered` from the memories it
//! cites, and the first page always carries them. What follows is proof the
//! answer does not stand on: evidence ranked below the cited core and the
//! relations around it. Offering it as a continuation made every answered
//! ask a walk through all of it — measured on the P1–P6 set, the walk cost
//! +74 % tokens on synth 10^3 and +11 % on B-real over the same binary with
//! the gate off, for the same answer. So an answered first page carries what
//! fits, names how much more there is in `projection.more_on_request`, and
//! offers no continuation. `budget.detail: "full"` asks for all of it and
//! pages it as before.
//!
//! `partial` and `unknown` keep their continuation. A PARTIAL's expansion
//! may state what its cited core lacked, and an UNKNOWN's proof is what was
//! read instead of an answer; both page little (1.3 and 1.1–2.2 pages
//! measured), so withholding it would save little and could hide the part
//! that was not found. Without the gate there is no `answer_status` and
//! nothing changes.

use serde_json::Value;

use super::budget::Detail;

/// The page's warning when the rest of an answered proof was withheld.
/// Never longer than the longest warning the planning envelope reserves.
pub(super) const MORE_ON_REQUEST: &str = "answer settled by its cited core; projection.more_on_request counts the rest of its proof: repeat with budget.detail \"full\" to page it";

/// The projection counter of what an answered first page left on request.
pub(super) const MORE_ON_REQUEST_KEY: &str = "more_on_request";

/// Whether the rendered answer is one whose proof beyond the first page is
/// on request: the gate answered it.
pub(super) fn settles_on_first_page(value: &Value) -> bool {
    value.get("answer_status").and_then(Value::as_str) == Some("answered")
}

/// Whether this page withholds that proof: an answered first page read at a
/// detail short of `full`. A caller that sends a cursor already chose to
/// page, and gets the page it asked for.
pub(super) fn withholds(settled: bool, detail: Detail, offset: usize) -> bool {
    settled && detail < Detail::Full && offset == 0
}
