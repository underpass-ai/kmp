use serde_json::Value;

use crate::auth::Identity;
pub use crate::domain::authorization_error::AuthorizationError;

pub const READ_SCOPE: &str = "kmp:read";
pub const WRITE_SCOPE: &str = "kmp:write";
pub const RAW_SCOPE: &str = "kmp:inspect:raw";
pub const ALL_ABOUTS_SCOPE: &str = "kmp:all-abouts";

pub fn authorize(identity: &Identity, request: &Value) -> Result<(), AuthorizationError> {
    match request.get("method").and_then(Value::as_str) {
        Some("initialize" | "notifications/initialized" | "tools/list") => Ok(()),
        Some("tools/call") => authorize_tool_call(identity, request),
        Some(_) | None => Ok(()),
    }
}

fn authorize_tool_call(identity: &Identity, request: &Value) -> Result<(), AuthorizationError> {
    let params = request.get("params").and_then(Value::as_object);
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = canonical_tool_name(name);
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .unwrap_or(&Value::Null);

    match name {
        "kmp_ingest" | "kmp_write_memory" => require_scope(identity, WRITE_SCOPE)?,
        "kmp_wake" | "kmp_ask" | "kmp_goto" | "kmp_near" | "kmp_rewind" | "kmp_forward"
        | "kmp_trace" | "kmp_inspect" => require_scope(identity, READ_SCOPE)?,
        _ => return Ok(()),
    }

    authorize_about(identity, arguments)?;
    authorize_dimensions(identity, arguments)?;
    authorize_write_scope_ids(identity, name, arguments)?;

    if requests_raw(name, arguments) {
        require_scope(identity, RAW_SCOPE)?;
    }

    match name {
        "kmp_ingest" => authorize_ingest_refs(identity, arguments)?,
        "kmp_trace" => {
            authorize_ref(
                identity,
                arguments.get("from").and_then(Value::as_str),
                None,
            )?;
            authorize_ref(identity, arguments.get("to").and_then(Value::as_str), None)?;
        }
        "kmp_inspect" => {
            authorize_ref(identity, arguments.get("ref").and_then(Value::as_str), None)?;
        }
        "kmp_write_memory" => {
            authorize_ref(
                identity,
                arguments.pointer("/current/ref").and_then(Value::as_str),
                arguments.get("about").and_then(Value::as_str),
            )?;
            authorize_ref(
                identity,
                arguments
                    .pointer("/semantic_delta/ref")
                    .and_then(Value::as_str),
                arguments.get("about").and_then(Value::as_str),
            )?;
            authorize_write_connections(identity, arguments)?;
        }
        _ => {}
    }
    Ok(())
}

fn canonical_tool_name(name: &str) -> &str {
    if matches!(name, "kernel_remember" | "kernel_ingest_context") {
        return "kmp_ingest";
    }
    name.strip_prefix("kernel_")
        .map(|suffix| match suffix {
            "ingest" => "kmp_ingest",
            "write_memory" => "kmp_write_memory",
            "wake" => "kmp_wake",
            "ask" => "kmp_ask",
            "goto" => "kmp_goto",
            "near" => "kmp_near",
            "rewind" => "kmp_rewind",
            "forward" => "kmp_forward",
            "trace" => "kmp_trace",
            "inspect" => "kmp_inspect",
            _ => name,
        })
        .unwrap_or(name)
}

fn authorize_write_scope_ids(
    identity: &Identity,
    name: &str,
    arguments: &Value,
) -> Result<(), AuthorizationError> {
    let mut requested = Vec::new();
    match name {
        "kmp_ingest" => {
            if let Some(dimensions) = arguments
                .pointer("/memory/dimensions")
                .and_then(Value::as_array)
            {
                requested.extend(
                    dimensions
                        .iter()
                        .filter_map(|dimension| dimension.get("id"))
                        .filter_map(Value::as_str),
                );
            }
        }
        "kmp_write_memory" => {
            if let Some(scope) = arguments.get("scope").and_then(Value::as_object) {
                requested.extend(scope.values().filter_map(Value::as_str));
            }
        }
        _ => return Ok(()),
    }

    for scope_id in requested {
        require_allowed(
            identity.scope_ids.contains("*") || identity.scope_ids.contains(scope_id),
            || format!("write scope id `{scope_id}` is outside the token grant"),
        )?;
    }
    Ok(())
}

fn require_scope(
    identity: &Identity,
    required_scope: &'static str,
) -> Result<(), AuthorizationError> {
    if identity.has_scope(required_scope) {
        Ok(())
    } else {
        Err(AuthorizationError::missing_scope(required_scope))
    }
}

fn authorize_about(identity: &Identity, arguments: &Value) -> Result<(), AuthorizationError> {
    if let Some(about) = arguments.get("about").and_then(Value::as_str) {
        require_allowed(
            identity.abouts.contains("*") || identity.abouts.contains(about),
            || format!("about `{about}` is outside the token grant"),
        )?;
    }
    Ok(())
}

fn authorize_dimensions(identity: &Identity, arguments: &Value) -> Result<(), AuthorizationError> {
    let Some(dimensions) = arguments.get("dimensions").and_then(Value::as_object) else {
        return Ok(());
    };

    if dimensions.get("scope").and_then(Value::as_str) == Some("all_abouts") {
        require_scope(identity, ALL_ABOUTS_SCOPE)?;
    }
    if let Some(abouts) = dimensions.get("abouts").and_then(Value::as_array) {
        for about in abouts.iter().filter_map(Value::as_str) {
            require_allowed(
                identity.abouts.contains("*") || identity.abouts.contains(about),
                || format!("about `{about}` is outside the token grant"),
            )?;
        }
    }
    if let Some(scope_ids) = dimensions.get("scope_ids").and_then(Value::as_array) {
        for scope_id in scope_ids.iter().filter_map(Value::as_str) {
            require_allowed(
                identity.scope_ids.contains("*") || identity.scope_ids.contains(scope_id),
                || format!("scope id `{scope_id}` is outside the token grant"),
            )?;
        }
    }
    Ok(())
}

fn requests_raw(name: &str, arguments: &Value) -> bool {
    if name == "kmp_inspect" {
        return arguments
            .pointer("/include/raw")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    }
    matches!(name, "kmp_goto" | "kmp_near" | "kmp_rewind" | "kmp_forward")
        && arguments
            .pointer("/include/raw_refs")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn authorize_write_connections(
    identity: &Identity,
    arguments: &Value,
) -> Result<(), AuthorizationError> {
    let about = arguments.get("about").and_then(Value::as_str);
    if let Some(connections) = arguments.get("connect_to").and_then(Value::as_array) {
        for reference in connections
            .iter()
            .filter_map(|connection| connection.get("ref"))
            .filter_map(Value::as_str)
        {
            authorize_ref(identity, Some(reference), about)?;
        }
    }
    Ok(())
}

fn authorize_ingest_refs(identity: &Identity, arguments: &Value) -> Result<(), AuthorizationError> {
    let about = arguments.get("about").and_then(Value::as_str);
    let Some(memory) = arguments.get("memory") else {
        return Ok(());
    };

    if let Some(entries) = memory.get("entries").and_then(Value::as_array) {
        for entry in entries {
            authorize_ref(identity, entry.get("id").and_then(Value::as_str), about)?;
        }
    }

    if let Some(relations) = memory.get("relations").and_then(Value::as_array) {
        for relation in relations {
            for field in ["from", "to", "decision_id", "caused_by_node_id"] {
                authorize_ref(identity, relation.get(field).and_then(Value::as_str), about)?;
            }
        }
    }

    if let Some(evidence) = memory.get("evidence").and_then(Value::as_array) {
        for item in evidence {
            authorize_ref(identity, item.get("id").and_then(Value::as_str), about)?;
            if let Some(supports) = item.get("supports").and_then(Value::as_array) {
                for reference in supports.iter().filter_map(Value::as_str) {
                    authorize_ref(identity, Some(reference), about)?;
                }
            }
        }
    }

    Ok(())
}

fn authorize_ref(
    identity: &Identity,
    reference: Option<&str>,
    current_about: Option<&str>,
) -> Result<(), AuthorizationError> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let current_about_grants =
        current_about.is_some_and(|about| reference_belongs_to_about(reference, about));
    let prefix_grants = identity.ref_prefixes.contains("*")
        || identity
            .ref_prefixes
            .iter()
            .any(|prefix| reference.starts_with(prefix));
    require_allowed(current_about_grants || prefix_grants, || {
        format!("ref `{reference}` is outside the token grant")
    })
}

fn reference_belongs_to_about(reference: &str, about: &str) -> bool {
    reference == about
        || reference
            .strip_prefix(about)
            .is_some_and(|suffix| suffix.starts_with(':'))
        || reference
            .strip_prefix("evidence:")
            .and_then(|reference| reference.strip_prefix(about))
            .is_some_and(|suffix| suffix.starts_with(':'))
        || reference
            .strip_prefix("about:")
            .and_then(|reference| reference.strip_prefix(about))
            .is_some_and(|suffix| suffix.starts_with(":dimension:"))
}

fn require_allowed(
    allowed: bool,
    reason: impl FnOnce() -> String,
) -> Result<(), AuthorizationError> {
    if allowed {
        Ok(())
    } else {
        Err(AuthorizationError::denied(reason()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    fn identity(scopes: &[&str]) -> Identity {
        Identity {
            subject: "agent-1".to_string(),
            workspace: Some("underpass".to_string()),
            scopes: scopes.iter().map(|value| (*value).to_string()).collect(),
            abouts: BTreeSet::from(["project:kmp".to_string()]),
            scope_ids: BTreeSet::from(["timeline:kmp".to_string()]),
            ref_prefixes: BTreeSet::from(["project:kmp:".to_string()]),
        }
    }

    fn call(name: &str, arguments: Value) -> Value {
        json!({"method":"tools/call","params":{"name":name,"arguments":arguments}})
    }

    #[test]
    fn enforces_about_and_dimension_grants() {
        let actor = identity(&[READ_SCOPE]);
        assert!(authorize(&actor, &call("kmp_wake", json!({"about":"project:kmp"}))).is_ok());
        assert!(authorize(&actor, &call("kmp_wake", json!({"about":"project:other"}))).is_err());
        assert!(
            authorize(
                &actor,
                &call(
                    "kmp_wake",
                    json!({
                        "about":"project:kmp", "dimensions":{"scope_ids":["timeline:other"]}
                    })
                )
            )
            .is_err()
        );
    }

    #[test]
    fn all_abouts_and_raw_reads_need_distinct_scopes() {
        let actor = identity(&[READ_SCOPE]);
        assert_eq!(
            authorize(
                &actor,
                &call(
                    "kmp_ask",
                    json!({
                        "about":"project:kmp", "dimensions":{"scope":"all_abouts"}
                    })
                )
            )
            .expect_err("all abouts denied")
            .required_scope,
            Some(ALL_ABOUTS_SCOPE)
        );
        assert_eq!(
            authorize(
                &actor,
                &call(
                    "kmp_inspect",
                    json!({
                        "ref":"project:kmp:decision:1", "include":{"raw":true}
                    })
                )
            )
            .expect_err("raw denied")
            .required_scope,
            Some(RAW_SCOPE)
        );
    }

    #[test]
    fn writer_cannot_connect_outside_granted_ref_prefixes() {
        let actor = identity(&[WRITE_SCOPE]);
        let denied = call(
            "kmp_write_memory",
            json!({
                "about":"project:kmp", "connect_to":[{"ref":"project:secret:decision:1"}]
            }),
        );
        assert!(authorize(&actor, &denied).is_err());
    }

    #[test]
    fn legacy_ingest_alias_cannot_bypass_write_authorization() {
        let actor = identity(&[READ_SCOPE]);
        let request = call(
            "kernel_remember",
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}],"entries":[]}
            }),
        );
        assert_eq!(
            authorize(&actor, &request)
                .expect_err("legacy ingest alias must require write scope")
                .required_scope,
            Some(WRITE_SCOPE)
        );
    }

    #[test]
    fn writes_are_bounded_to_granted_scope_ids_and_refs() {
        let actor = identity(&[WRITE_SCOPE]);
        assert!(
            authorize(
                &actor,
                &call(
                    "kmp_ingest",
                    json!({
                        "about":"project:kmp",
                        "memory":{"dimensions":[{"id":"timeline:other"}],"entries":[]}
                    })
                )
            )
            .is_err()
        );
        assert!(
            authorize(
                &actor,
                &call(
                    "kmp_write_memory",
                    json!({
                        "about":"project:kmp",
                        "scope":{"process":"timeline:other"},
                        "current":{"ref":"project:other:decision:1"}
                    })
                )
            )
            .is_err()
        );
    }

    #[test]
    fn ingest_authorizes_every_caller_chosen_ref() {
        let actor = identity(&[WRITE_SCOPE]);
        let local = json!({
            "about":"project:kmp",
            "memory":{
                "dimensions":[{"id":"timeline:kmp"}],
                "entries":[{"id":"project:kmp:entry:1"}],
                "relations":[{
                    "from":"project:kmp:entry:1",
                    "to":"project:kmp:entry:2",
                    "decision_id":"project:kmp:decision:1",
                    "caused_by_node_id":"project:kmp:finding:1"
                }],
                "evidence":[{
                    "id":"evidence:project:kmp:entry:1:current",
                    "supports":["project:kmp:entry:1"]
                }]
            }
        });
        assert!(authorize(&actor, &call("kmp_ingest", local)).is_ok());

        let canonical_dimension = json!({
            "about":"project:kmp",
            "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                "relations":[{
                    "from":"about:project:kmp:dimension:timeline:kmp",
                    "to":"project:kmp:entry:1"
                }]}
        });
        assert!(
            authorize(&actor, &call("kmp_ingest", canonical_dimension)).is_ok(),
            "canonical evidence and dimension refs owned by the exact about must remain authorized"
        );

        let foreign_payloads = [
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}],
                    "entries":[{"id":"project:other:entry:1"}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "relations":[{"from":"project:other:entry:1", "to":"project:kmp:entry:1"}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "relations":[{"from":"project:kmp:entry:1", "to":"project:other:entry:1"}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "evidence":[{"id":"project:other:evidence:1", "supports":[]}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "evidence":[{"id":"project:kmp:evidence:1",
                        "supports":["project:other:entry:1"]}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "relations":[{"from":"project:kmp:entry:1", "to":"project:kmp:entry:2",
                        "decision_id":"project:other:decision:1"}]}
            }),
            json!({
                "about":"project:kmp",
                "memory":{"dimensions":[{"id":"timeline:kmp"}], "entries":[],
                    "relations":[{"from":"project:kmp:entry:1", "to":"project:kmp:entry:2",
                        "caused_by_node_id":"project:other:finding:1"}]}
            }),
        ];

        for payload in foreign_payloads {
            let error = authorize(&actor, &call("kmp_ingest", payload))
                .expect_err("foreign ingest ref must be denied");
            assert!(
                error.reason.contains("project:other:"),
                "unexpected denial: {}",
                error.reason
            );
        }
    }
}
