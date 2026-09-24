//! The one time-navigation verb and the move it makes.
//!
//! `kmp_time` replaced four tools whose schemas differed only in the name of
//! their cursor. The move names which of those four navigations a call makes
//! and which cursor it takes; the argument shape is otherwise shared. Hosts do
//! not reliably evaluate the per-move conditions in the advertised schema, so
//! every call is checked here before any backend reads it.
use serde_json::{Value, json};

use crate::serving::ToolError;

/// The time-navigation tool name, the only one this build advertises.
pub(crate) const TIME_TOOL: &str = "kmp_time";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeMove {
    Rewind,
    Forward,
    Goto,
    Near,
}

impl TimeMove {
    pub(crate) const ALL: [Self; 4] = [Self::Rewind, Self::Forward, Self::Goto, Self::Near];

    /// Every cursor any move accepts; each move admits exactly one of them.
    const CURSORS: [&'static str; 3] = ["from", "at", "around"];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Rewind => "rewind",
            Self::Forward => "forward",
            Self::Goto => "goto",
            Self::Near => "near",
        }
    }

    /// The move a temporal response's `temporal.direction` came from.
    pub(crate) fn from_direction(direction: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == direction)
    }

    /// The cursor argument this move reads.
    pub(crate) fn cursor_key(self) -> &'static str {
        match self {
            Self::Rewind | Self::Forward => "from",
            Self::Goto => "at",
            Self::Near => "around",
        }
    }

    /// Rewind and Forward can start from an interval alone.
    fn starts_from_interval(self) -> bool {
        matches!(self, Self::Rewind | Self::Forward)
    }

    /// The executable action for this move: the arguments with `move` set.
    pub(crate) fn action(self, mut arguments: Value) -> Value {
        if let Some(object) = arguments.as_object_mut() {
            object.insert("move".into(), json!(self.as_str()));
        }
        json!({"tool": TIME_TOOL, "arguments": arguments})
    }

    /// The tool a retired per-move name became, for an actionable refusal.
    pub(crate) fn retired(name: &str) -> Option<Self> {
        let suffix = name
            .strip_prefix("kmp_")
            .or_else(|| name.strip_prefix("kernel_"))?;
        Self::from_direction(suffix)
    }

    /// Reads and checks the move and its cursor. Every refusal names the
    /// field and the call that would have been accepted.
    pub(crate) fn from_arguments(arguments: &Value) -> Result<Self, ToolError> {
        let allowed = Self::ALL.map(Self::as_str);
        let requested = arguments.get("move");
        let Some(movement) = requested
            .and_then(Value::as_str)
            .and_then(Self::from_direction)
        else {
            let reason = match requested {
                None => "kmp_time requires `move`".to_string(),
                Some(value) => format!("kmp_time has no move {value}"),
            };
            return Err(ToolError::invalid_argument(format!(
                "{reason}: one of rewind (newest first, before `from` or within `interval`), \
                 forward (oldest first, after `from` or within `interval`), goto (state at `at`) \
                 or near (neighbourhood around `around`)."
            ))
            .with_feedback(
                json!({"code":"TIME_INVALID_MOVE","severity":"error","field":"move",
                "allowed":allowed,"action":null}),
            ));
        };
        let cursor = movement.cursor_key();
        if let Some(foreign) = Self::CURSORS
            .into_iter()
            .find(|key| *key != cursor && arguments.get(*key).is_some())
        {
            let owner = Self::ALL
                .into_iter()
                .filter(|candidate| candidate.cursor_key() == foreign)
                .map(Self::as_str)
                .collect::<Vec<_>>()
                .join(" and ");
            return Err(ToolError::invalid_argument(format!(
                "move {} takes its cursor as `{cursor}`; `{foreign}` belongs to {owner}. \
                 Rename the cursor or choose the move that reads it.",
                movement.as_str()
            ))
            .with_feedback(json!({"code":"TIME_CURSOR_MISMATCH","severity":"error",
                "field":foreign,"expected":cursor,"action":null})));
        }
        let has_interval = arguments.get("interval").is_some();
        if arguments.get(cursor).is_none() && !(movement.starts_from_interval() && has_interval) {
            let accepted = if movement.starts_from_interval() {
                format!("`{cursor}` (time, sequence or ref) or `interval`")
            } else {
                format!("`{cursor}` (time, sequence or ref)")
            };
            return Err(ToolError::invalid_argument(format!(
                "move {} requires {accepted}.",
                movement.as_str()
            ))
            .with_feedback(json!({"code":"TIME_MISSING_CURSOR","severity":"error",
                "field":cursor,"action":null})));
        }
        Ok(movement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(arguments: Value) -> ToolError {
        TimeMove::from_arguments(&arguments).expect_err("refused")
    }

    #[test]
    fn each_move_reads_its_own_cursor() {
        for (movement, cursor) in [
            ("rewind", "from"),
            ("forward", "from"),
            ("goto", "at"),
            ("near", "around"),
        ] {
            let arguments = json!({"about":"a","move":movement,cursor:{"ref":"r"}});
            let parsed = TimeMove::from_arguments(&arguments).expect(movement);
            assert_eq!(parsed.as_str(), movement);
            assert_eq!(parsed.cursor_key(), cursor);
        }
    }

    #[test]
    fn only_rewind_and_forward_start_from_an_interval() {
        let interval = json!({"start":"2026-09-01T00:00:00Z"});
        for movement in ["rewind", "forward"] {
            assert!(
                TimeMove::from_arguments(&json!({"about":"a","move":movement,"interval":interval}))
                    .is_ok()
            );
        }
        for movement in ["goto", "near"] {
            let error = refusal(json!({"about":"a","move":movement,"interval":interval}));
            assert_eq!(error.feedback[0]["code"], "TIME_MISSING_CURSOR");
        }
    }

    #[test]
    fn refusals_name_the_field_and_the_accepted_call() {
        let missing = refusal(json!({"about":"a"}));
        assert_eq!(missing.feedback[0]["code"], "TIME_INVALID_MOVE");
        assert!(missing.message.contains("requires `move`"));
        let unknown = refusal(json!({"about":"a","move":"sideways"}));
        assert_eq!(unknown.feedback[0]["allowed"][3], "near");
        let crossed = refusal(json!({"about":"a","move":"goto","from":{"ref":"r"}}));
        assert_eq!(crossed.feedback[0]["field"], "from");
        assert!(
            crossed
                .message
                .contains("`from` belongs to rewind and forward")
        );
        let both =
            refusal(json!({"about":"a","move":"near","around":{"ref":"r"},"at":{"ref":"r"}}));
        assert_eq!(both.feedback[0]["code"], "TIME_CURSOR_MISMATCH");
        let bare = refusal(json!({"about":"a","move":"rewind"}));
        assert!(bare.message.contains("or `interval`"));
    }

    #[test]
    fn retired_names_point_at_their_move() {
        assert_eq!(TimeMove::retired("kmp_goto"), Some(TimeMove::Goto));
        assert_eq!(TimeMove::retired("kernel_rewind"), Some(TimeMove::Rewind));
        assert_eq!(TimeMove::retired("kmp_wake"), None);
        let action = TimeMove::Forward.action(json!({"about":"a","from":{"ref":"r"}}));
        assert_eq!(action["tool"], "kmp_time");
        assert_eq!(action["arguments"]["move"], "forward");
    }
}
