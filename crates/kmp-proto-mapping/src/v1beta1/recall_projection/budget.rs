//! The recall budget the caller asked for: the byte ceiling, the advisory
//! token hint, the detail tier and the page size, read from the arguments.

use serde_json::Value;

pub const DEFAULT_MAX_BYTES: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Detail {
    Compact,
    Balanced,
    Full,
}

/// The longest `budget.detail` word a request can carry.
pub(super) const LONGEST_DETAIL: &str = "balanced";

impl Detail {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "compact" => Ok(Self::Compact),
            "balanced" => Ok(Self::Balanced),
            "full" => Ok(Self::Full),
            other => Err(format!("invalid budget.detail `{other}`")),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Balanced => "balanced",
            Self::Full => "full",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ProjectionBudget {
    pub(super) token_limit: u32,
    pub(super) byte_limit: usize,
    pub(super) detail: Detail,
    pub(super) max_entries: Option<usize>,
    pub(super) page_entries: usize,
    /// On a continuation, return the stable core again instead of only the
    /// new expansion items: the rehydration path for a caller that lost the
    /// first page.
    pub(super) repeat_core: bool,
}

impl ProjectionBudget {
    pub(super) fn from_arguments(arguments: &Value, default_tokens: u32) -> Result<Self, String> {
        let budget = arguments.get("budget").and_then(Value::as_object);
        let token_limit = budget
            .and_then(|budget| budget.get("tokens"))
            .and_then(Value::as_u64)
            .and_then(|tokens| u32::try_from(tokens).ok())
            .filter(|tokens| *tokens > 0)
            .unwrap_or(default_tokens);
        let byte_limit = match budget.and_then(|budget| budget.get("max_bytes")) {
            None => DEFAULT_MAX_BYTES,
            Some(value) => value
                .as_u64()
                .and_then(|bytes| usize::try_from(bytes).ok())
                .filter(|bytes| *bytes >= 512)
                .ok_or_else(|| "budget.max_bytes must be an integer of at least 512".to_string())?,
        };
        let detail = Detail::parse(
            budget
                .and_then(|budget| budget.get("detail"))
                .and_then(Value::as_str)
                .unwrap_or("balanced"),
        )?;
        let max_entries = budget
            .and_then(|budget| budget.get("max_entries"))
            .and_then(Value::as_u64)
            .and_then(|entries| usize::try_from(entries).ok())
            .filter(|entries| *entries > 0);
        let page = match arguments.get("page") {
            None => None,
            Some(Value::Object(page)) => Some(page),
            Some(_) => return Err("page must be an object".to_string()),
        };
        let page_entries = match page.and_then(|page| page.get("entries")) {
            None => usize::MAX,
            Some(value) => value
                .as_u64()
                .and_then(|entries| usize::try_from(entries).ok())
                .filter(|entries| *entries > 0)
                .ok_or_else(|| "page.entries must be a positive integer".to_string())?,
        };
        if let Some(cursor) = page.and_then(|page| page.get("cursor"))
            && !cursor
                .as_str()
                .is_some_and(|cursor| !cursor.trim().is_empty())
        {
            return Err("page.cursor must be a non-empty opaque string".to_string());
        }
        let repeat_core = match page.and_then(|page| page.get("repeat_core")) {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| "page.repeat_core must be a boolean".to_string())?,
        };
        Ok(Self {
            token_limit,
            byte_limit,
            detail,
            max_entries,
            page_entries,
            repeat_core,
        })
    }
}
