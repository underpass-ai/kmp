//! Where a tool call came from: the host session, whether it pages an
//! earlier call, and the guidance context it ran in. Read before the call's
//! arguments are resolved, because a continuation resolves into the call it
//! continues.

use serde_json::Value;

use super::call_fingerprints::CallFingerprints;
use super::fingerprint_salt::FingerprintSalt;
use super::mcp_client::McpClient;
use super::recorders::canonical_move;

#[derive(Debug, Default)]
pub(crate) struct CallOrigin {
    pub(crate) client: Option<McpClient>,
    /// `Some` for the reads that page: whether this call is a page of an
    /// earlier one (a `continuation` handle or a `page.cursor`) rather than
    /// a new call.
    pub(crate) is_continuation: Option<bool>,
    /// Kept only to be fingerprinted; never logged as itself.
    context_id: Option<String>,
}

impl CallOrigin {
    /// `arguments` as the host sent them, before a continuation resolves.
    pub(crate) fn read(name: &str, arguments: &Value, client: Option<McpClient>) -> Self {
        let pages = crate::guidance::ReadContinuation::supports_read(canonical_move(name));
        let continues = arguments.get("continuation").is_some()
            || arguments
                .pointer("/page/cursor")
                .and_then(Value::as_str)
                .is_some_and(|cursor| !cursor.is_empty());
        Self {
            client,
            is_continuation: pages.then_some(continues),
            context_id: arguments
                .get("context_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string),
        }
    }

    /// Fingerprints of an ask's question or a wake's intent, from the
    /// resolved `arguments` (a page carries its first call's question), and
    /// of the context id. Nothing without a salt or for other tools.
    pub(crate) fn fingerprints(
        &self,
        name: &str,
        arguments: &Value,
        salt: Option<&FingerprintSalt>,
    ) -> CallFingerprints {
        let Some(salt) = salt else {
            return CallFingerprints::default();
        };
        let subject = match canonical_move(name) {
            "kmp_ask" => Some(("question", "question")),
            "kmp_wake" => Some(("intent", "intent")),
            _ => None,
        };
        let Some((domain, key)) = subject else {
            return CallFingerprints::default();
        };
        CallFingerprints {
            subject: arguments
                .get(key)
                .and_then(Value::as_str)
                .map(normalized)
                .filter(|text| !text.is_empty())
                .map(|text| salt.fingerprint(domain, &text)),
            context: self
                .context_id
                .as_deref()
                .map(|id| salt.fingerprint("context", id)),
        }
    }
}

/// Case and spacing do not make a different question.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::CallOrigin;
    use crate::serving::telemetry::fingerprint_salt::{FingerprintSalt, TELEMETRY_SALT_FILE};

    fn salt(dir: &tempfile::TempDir) -> FingerprintSalt {
        FingerprintSalt::load_or_create(&dir.path().join(TELEMETRY_SALT_FILE)).expect("salt")
    }

    #[test]
    fn a_page_is_told_apart_from_a_new_call() {
        let new = CallOrigin::read("kmp_ask", &json!({"question": "q"}), None);
        let handle = CallOrigin::read("kmp_ask", &json!({"continuation": "c1"}), None);
        let cursor = CallOrigin::read("kmp_wake", &json!({"page": {"cursor": "k"}}), None);
        let blank = CallOrigin::read("kmp_wake", &json!({"page": {"cursor": ""}}), None);
        let write = CallOrigin::read("kmp_write_memory", &json!({}), None);

        assert_eq!(new.is_continuation, Some(false));
        assert_eq!(handle.is_continuation, Some(true));
        assert_eq!(cursor.is_continuation, Some(true));
        assert_eq!(blank.is_continuation, Some(false));
        assert_eq!(write.is_continuation, None);
    }

    #[test]
    fn fingerprints_follow_the_question_and_context_not_their_spelling() {
        let dir = tempfile::tempdir().expect("dir");
        let salt = salt(&dir);
        let origin = CallOrigin::read("kmp_ask", &json!({"context_id": "ctx-1"}), None);

        let one = origin.fingerprints(
            "kmp_ask",
            &json!({"question": "Who approved it?"}),
            Some(&salt),
        );
        let two = origin.fingerprints(
            "kmp_ask",
            &json!({"question": "  who   APPROVED it? "}),
            Some(&salt),
        );
        let other = origin.fingerprints(
            "kmp_ask",
            &json!({"question": "Who wrote it?"}),
            Some(&salt),
        );

        assert_eq!(one, two);
        assert_ne!(one.subject, other.subject);
        assert_eq!(one.context, other.context);
        assert!(one.context.is_some());
        assert_ne!(one.subject.as_deref(), Some("who approved it?"));
    }

    #[test]
    fn no_salt_other_tools_and_absent_text_print_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let salt = salt(&dir);
        let origin = CallOrigin::read("kmp_wake", &json!({}), None);

        assert_eq!(
            origin.fingerprints("kmp_ask", &json!({"question": "q"}), None),
            Default::default()
        );
        assert_eq!(
            origin.fingerprints("kmp_inspect", &json!({"question": "q"}), Some(&salt)),
            Default::default()
        );
        let wake = origin.fingerprints("kmp_wake", &json!({"intent": " "}), Some(&salt));
        assert_eq!(wake.subject, None);
        assert_eq!(wake.context, None);
        let intent = origin.fingerprints("kmp_wake", &json!({"intent": "resume"}), Some(&salt));
        assert!(intent.subject.is_some());
    }
}
