//! Where a tool call came from: the host session and whether it pages an
//! earlier call. Read before the call's arguments are resolved, because a
//! continuation resolves into the call it continues.

use serde_json::Value;

use super::call_fingerprints::CallFingerprints;
use super::mcp_client::McpClient;
use super::recorders::canonical_move;
use kmp_observability::FingerprintSalt;

#[derive(Debug, Default)]
pub(crate) struct CallOrigin {
    pub(crate) client: Option<McpClient>,
    /// Whether this call is a page of an earlier one (a `continuation`
    /// handle or a `page.cursor`) rather than a new call; `None` only for
    /// a call nothing was read of.
    pub(crate) is_continuation: Option<bool>,
}

impl CallOrigin {
    /// `arguments` as the host sent them, before a continuation resolves.
    pub(crate) fn read(arguments: &Value, client: Option<McpClient>) -> Self {
        let continues = arguments.get("continuation").is_some()
            || arguments
                .pointer("/page/cursor")
                .and_then(Value::as_str)
                .is_some_and(|cursor| !cursor.is_empty());
        Self {
            client,
            is_continuation: Some(continues),
        }
    }

    /// The fingerprint of an ask's question or a wake's intent, from the
    /// resolved `arguments` (a page carries its first call's question).
    /// Nothing without a salt.
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
            "kmp_ask" => Some("question"),
            "kmp_wake" => Some("intent"),
            _ => None,
        };
        CallFingerprints {
            subject: subject.and_then(|key| {
                let text = arguments.get(key).and_then(Value::as_str)?;
                salt.fingerprint_text(key, text)
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::CallOrigin;
    use kmp_observability::{FingerprintSalt, TELEMETRY_SALT_FILE};

    fn salt(dir: &tempfile::TempDir) -> FingerprintSalt {
        FingerprintSalt::load_or_create(&dir.path().join(TELEMETRY_SALT_FILE)).expect("salt")
    }

    #[test]
    fn a_page_is_told_apart_from_a_new_call() {
        let new = CallOrigin::read(&json!({"question": "q"}), None);
        let handle = CallOrigin::read(&json!({"continuation": "c1"}), None);
        let cursor = CallOrigin::read(&json!({"page": {"cursor": "k"}}), None);
        let blank = CallOrigin::read(&json!({"page": {"cursor": ""}}), None);
        let write = CallOrigin::read(&json!({}), None);

        assert_eq!(new.is_continuation, Some(false));
        assert_eq!(handle.is_continuation, Some(true));
        assert_eq!(cursor.is_continuation, Some(true));
        assert_eq!(blank.is_continuation, Some(false));
        assert_eq!(write.is_continuation, Some(false));
    }

    #[test]
    fn fingerprints_follow_the_question_not_its_spelling() {
        let dir = tempfile::tempdir().expect("dir");
        let salt = salt(&dir);
        let origin = CallOrigin::read(&json!({}), None);

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
        assert_ne!(one.subject.as_deref(), Some("who approved it?"));
    }

    #[test]
    fn no_salt_other_tools_and_absent_text_print_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let salt = salt(&dir);
        let origin = CallOrigin::read(&json!({}), None);

        assert_eq!(
            origin.fingerprints("kmp_ask", &json!({"question": "q"}), None),
            Default::default()
        );
        let inspect = origin.fingerprints("kmp_inspect", &json!({"question": "q"}), Some(&salt));
        assert_eq!(inspect.subject, None);
        let write = origin.fingerprints("kmp_write_memory", &json!({}), Some(&salt));
        assert_eq!(write.subject, None);
        let wake = origin.fingerprints("kmp_wake", &json!({"intent": " "}), Some(&salt));
        assert_eq!(wake.subject, None);
        let intent = origin.fingerprints("kmp_wake", &json!({"intent": "resume"}), Some(&salt));
        assert!(intent.subject.is_some());
    }
}
