//! MCP sessions of the legacy HTTP dialect, keyed by the `Mcp-Session-Id`
//! this gateway mints on `initialize`. Each keeps what its host negotiated
//! (client and MCP Apps), so concurrent hosts never read each other's. A
//! session belongs to the subject that opened it; any other subject, an
//! unknown or forgotten id, or no id is no session: the gateway answers 404
//! and the client initializes again (MCP Streamable HTTP).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use kmp_mcp::McpSession;

/// Sessions kept at once; the oldest is forgotten first.
pub const MAX_SESSIONS: usize = 4096;
pub const SESSION_HEADER: &str = "mcp-session-id";

#[derive(Default)]
pub struct HttpSessions {
    inner: Mutex<Registry>,
}

#[derive(Default)]
struct Registry {
    sessions: HashMap<String, (String, Arc<McpSession>)>,
    order: VecDeque<String>,
}

impl HttpSessions {
    /// A new session for `subject` and its id, to return in the
    /// `Mcp-Session-Id` header. `None` when no randomness is available: the
    /// request is then served in an anonymous session.
    pub fn open(&self, subject: &str) -> Option<(String, Arc<McpSession>)> {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).ok()?;
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let session = Arc::new(McpSession::new());
        let mut registry = self.inner.lock().ok()?;
        while registry.sessions.len() >= MAX_SESSIONS {
            let Some(oldest) = registry.order.pop_front() else {
                break;
            };
            registry.sessions.remove(&oldest);
        }
        registry
            .sessions
            .insert(id.clone(), (subject.to_string(), Arc::clone(&session)));
        registry.order.push_back(id.clone());
        Some((id, session))
    }

    /// The session `id` names if `subject` opened it; `None` otherwise.
    pub fn resume(&self, id: Option<&str>, subject: &str) -> Option<Arc<McpSession>> {
        let registry = self.inner.lock().ok()?;
        let (owner, session) = registry.sessions.get(id?)?;
        (owner == subject).then(|| Arc::clone(session))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{HttpSessions, MAX_SESSIONS};

    #[test]
    fn a_session_is_resumed_only_by_its_subject() {
        let sessions = HttpSessions::default();
        let (id, session) = sessions.open("alice").expect("session");
        assert_eq!(id.len(), 32);
        session.initialize(&json!({"params": {"clientInfo": {"name": "codex"}}}));

        let resumed = sessions.resume(Some(&id), "alice").expect("resumed");
        assert_eq!(resumed.client_name().as_deref(), Some("codex"));
        assert!(sessions.resume(Some(&id), "mallory").is_none());
        assert!(sessions.resume(Some("unknown"), "alice").is_none());
        assert!(sessions.resume(None, "alice").is_none());
    }

    #[test]
    fn the_oldest_session_is_forgotten_past_the_bound() {
        let sessions = HttpSessions::default();
        let (first, session) = sessions.open("alice").expect("first");
        session.initialize(&json!({"params": {"clientInfo": {"name": "first"}}}));
        for _ in 0..MAX_SESSIONS {
            sessions.open("alice").expect("session");
        }
        assert!(sessions.resume(Some(&first), "alice").is_none());
    }
}
