//! Several agents writing one about collide on its revision: the store moved
//! between planning and commit, so the attempt applied nothing (S1 of the 1.0
//! review). The kernel says such a write is safe to replay, so the server
//! replays it itself, planned afresh against the new revision, a bounded
//! number of times. The caller hears `retryable_conflict` only when every
//! attempt lost the race.

use std::time::Duration;

use serde_json::Value;

use crate::serving::ToolErrorCode;

/// Attempts in all, the first included.
pub(super) const ATTEMPTS: u32 = 12;

/// Whether a `tools/call` reply is a write the kernel refused only because
/// the store moved under it.
pub(super) fn lost_the_race(reply: &str) -> bool {
    serde_json::from_str::<Value>(reply).is_ok_and(|reply| {
        reply["result"]["structuredContent"]["error"]["code"]
            == ToolErrorCode::RetryableConflict.as_str()
    })
}

/// How long to wait before attempt `attempt` (1 = the first retry):
/// 4 ms, doubling, capped at 256 ms, with up to as much again of
/// jitter so writers that collided once do not collide again in step.
pub(super) fn pause_before(attempt: u32) -> Duration {
    let base = 2u64 << attempt.min(7);
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|now| u64::from(now.subsec_nanos()) % base.max(1))
        .unwrap_or(0);
    Duration::from_millis(base + jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_retryable_code_is_replayed() {
        let reply = |code: &str| {
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"isError":true,"structuredContent":{{"error":{{"code":"{code}","message":"m"}}}}}}}}"#
            )
        };
        assert!(lost_the_race(&reply("retryable_conflict")));
        assert!(!lost_the_race(&reply("conflict")));
        assert!(!lost_the_race(&reply("invalid_argument")));
        assert!(!lost_the_race(
            r#"{"jsonrpc":"2.0","id":1,"result":{"content":[]}}"#
        ));
        assert!(!lost_the_race("not json"));
    }

    #[test]
    fn the_pause_grows_and_stays_bounded() {
        assert!(pause_before(1) >= Duration::from_millis(4));
        assert!(pause_before(1) < Duration::from_millis(8));
        assert!(pause_before(30) < Duration::from_millis(512));
    }
}
