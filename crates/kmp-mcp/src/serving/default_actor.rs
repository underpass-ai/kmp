//! The writer a write is recorded under when the caller names none: the MCP
//! client as it named itself in `initialize`, or a fixed neutral name. An
//! explicit `actor` always wins. It is applied before the write is planned,
//! so its idempotency digest and any retained continuation carry it.

use serde_json::{Value, json};

use crate::serving::mcp_session::McpSession;

/// The name used when the session declared no client.
pub(crate) const FALLBACK_ACTOR: &str = "mcp-client";

/// `arguments` with the default actor when `tool` writes under one and none
/// was given; `None` when nothing changes.
pub(super) fn with_default_actor(
    tool: &str,
    arguments: &Value,
    session: &McpSession,
) -> Option<Value> {
    let writes = match tool {
        "kmp_write_memory" | "kmp_relabel" | "kmp_condense" => true,
        "kmp_curate" => arguments.get("mode").and_then(Value::as_str) == Some("apply"),
        _ => false,
    };
    if !writes || arguments.get("actor").is_some() || arguments.get("continuation").is_some() {
        return None;
    }
    let object = arguments.as_object()?;
    let name = session
        .client_name()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| FALLBACK_ACTOR.to_string());
    let mut defaulted = object.clone();
    defaulted.insert("actor".into(), json!(name));
    Some(Value::Object(defaulted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: Option<&str>) -> McpSession {
        let session = McpSession::new();
        if let Some(name) = name {
            session.initialize(&json!({"params":{"clientInfo":{"name":name,"version":"1"}}}));
        }
        session
    }

    #[test]
    fn an_omitted_actor_is_the_client_name_or_the_neutral_fallback() {
        let write = json!({"about":"a","memories":[]});
        let named = with_default_actor("kmp_write_memory", &write, &session(Some("claude-code")));
        assert_eq!(named.expect("defaulted")["actor"], "claude-code");
        let anonymous = with_default_actor("kmp_relabel", &write, &session(None));
        assert_eq!(anonymous.expect("defaulted")["actor"], FALLBACK_ACTOR);
        let blank = with_default_actor("kmp_condense", &write, &session(Some("  ")));
        assert_eq!(blank.expect("defaulted")["actor"], FALLBACK_ACTOR);
    }

    #[test]
    fn an_explicit_actor_reads_and_continuations_are_left_alone() {
        let any = session(Some("host"));
        let explicit = json!({"about":"a","actor":"me"});
        assert!(with_default_actor("kmp_write_memory", &explicit, &any).is_none());
        assert!(with_default_actor("kmp_wake", &json!({"about":"a"}), &any).is_none());
        let handle = json!({"continuation":"read_00000000000000000000000000000000"});
        assert!(with_default_actor("kmp_write_memory", &handle, &any).is_none());
        let review = json!({"about":"a","mode":"review"});
        assert!(with_default_actor("kmp_curate", &review, &any).is_none());
        let apply = json!({"about":"a","mode":"apply"});
        assert_eq!(
            with_default_actor("kmp_curate", &apply, &any).expect("apply")["actor"],
            "host"
        );
    }
}
