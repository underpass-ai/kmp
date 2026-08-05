use rehydration_ports::{ContextEventStore, ContextUpdatedEvent, IdempotentOutcome, PortError};

use crate::adapter::endpoint::ValkeyEndpoint;
use crate::adapter::io::{execute_eval_command, execute_get_command, execute_set_command};

const DEFAULT_KEY_PREFIX: &str = "rehydration:cmd";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValkeyContextEventStore {
    endpoint: ValkeyEndpoint,
}

impl ValkeyContextEventStore {
    pub fn new(runtime_state_uri: impl Into<String>) -> Result<Self, PortError> {
        let endpoint = ValkeyEndpoint::parse_with_default_key_prefix(
            runtime_state_uri.into(),
            "context event",
            DEFAULT_KEY_PREFIX,
        )?;
        Ok(Self { endpoint })
    }

    fn revision_key(&self, root_node_id: &str, role: &str) -> String {
        format!("{}:rev:{}:{}", self.endpoint.key_prefix, root_node_id, role)
    }

    fn hash_key(&self, root_node_id: &str, role: &str) -> String {
        format!(
            "{}:hash:{}:{}",
            self.endpoint.key_prefix, root_node_id, role
        )
    }

    fn idempotency_key(&self, key: &str) -> String {
        format!("{}:idem:{}", self.endpoint.key_prefix, key)
    }

    fn event_key(&self, root_node_id: &str, role: &str, revision: u64) -> String {
        format!(
            "{}:evt:{}:{}:{}",
            self.endpoint.key_prefix, root_node_id, role, revision
        )
    }
}

impl ContextEventStore for ValkeyContextEventStore {
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        let new_revision = expected_revision + 1;
        let rev_key = self.revision_key(&event.root_node_id, &event.role);
        let hash_key = self.hash_key(&event.root_node_id, &event.role);
        let event_key = self.event_key(&event.root_node_id, &event.role, new_revision);

        let event_json = serde_json::to_string(&event).map_err(|error| {
            PortError::InvalidState(format!("failed to serialize context event: {error}"))
        })?;

        // Atomic CAS via Lua: compare current revision, then set event + revision + hash.
        // If revision doesn't match, returns CONFLICT error.
        const CAS_SCRIPT: &str = r#"
            local current = redis.call('GET', KEYS[1])
            if current == false then current = '0' end
            if current ~= ARGV[1] then
                return redis.error_reply('CONFLICT: expected ' .. ARGV[1] .. ' got ' .. current)
            end
            redis.call('SET', KEYS[1], ARGV[2])
            redis.call('SET', KEYS[2], ARGV[3])
            redis.call('SET', KEYS[3], ARGV[4])
            return 'OK'
        "#;

        let expected_str = expected_revision.to_string();
        let new_str = new_revision.to_string();

        execute_eval_command(
            &self.endpoint,
            CAS_SCRIPT,
            &[&rev_key, &hash_key, &event_key],
            &[&expected_str, &new_str, &event.content_hash, &event_json],
        )
        .await?;

        if let Some(ref idem_key) = event.idempotency_key {
            let key = self.idempotency_key(idem_key);
            let value = encode_idempotent_value(
                new_revision,
                &event.content_hash,
                event.logical_digest.as_deref(),
            );
            execute_set_command(&self.endpoint, &key, &value, None).await?;
        }

        Ok(new_revision)
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        let key = self.revision_key(root_node_id, role);
        match execute_get_command(&self.endpoint, &key).await? {
            Some(value) => parse_revision(&value),
            None => Ok(0),
        }
    }

    async fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        let key = self.hash_key(root_node_id, role);
        execute_get_command(&self.endpoint, &key).await
    }

    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        let valkey_key = self.idempotency_key(key);
        match execute_get_command(&self.endpoint, &valkey_key).await? {
            Some(value) => parse_idempotent_outcome(&value).map(Some),
            None => Ok(None),
        }
    }
}

fn encode_idempotent_value(
    revision: u64,
    content_hash: &str,
    logical_digest: Option<&str>,
) -> String {
    match logical_digest {
        Some(digest) => format!("{revision}:{content_hash}:{digest}"),
        None => format!("{revision}:{content_hash}"),
    }
}

fn parse_idempotent_outcome(value: &str) -> Result<IdempotentOutcome, PortError> {
    let (revision_str, content_hash) = value
        .split_once(':')
        .ok_or_else(|| PortError::InvalidState(format!("malformed idempotency value: {value}")))?;
    let revision = revision_str.parse::<u64>().map_err(|error| {
        PortError::InvalidState(format!("invalid idempotency revision: {error}"))
    })?;
    let (content_hash, logical_digest) = match content_hash.split_once(':') {
        Some((hash, digest)) => (hash, Some(digest.to_string())),
        None => (content_hash, None),
    };
    Ok(IdempotentOutcome {
        revision,
        content_hash: content_hash.to_string(),
        logical_digest,
    })
}

fn parse_revision(value: &str) -> Result<u64, PortError> {
    value
        .parse::<u64>()
        .map_err(|error| PortError::InvalidState(format!("invalid revision value: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_construction_follows_prefix_pattern() {
        let store =
            ValkeyContextEventStore::new("redis://localhost:6379").expect("endpoint should parse");
        assert_eq!(
            store.revision_key("node-1", "developer"),
            "rehydration:cmd:rev:node-1:developer"
        );
        assert_eq!(
            store.hash_key("node-1", "developer"),
            "rehydration:cmd:hash:node-1:developer"
        );
        assert_eq!(
            store.idempotency_key("idem-abc"),
            "rehydration:cmd:idem:idem-abc"
        );
    }

    #[test]
    fn encode_and_parse_idempotent_value_roundtrips() {
        let encoded = encode_idempotent_value(42, "abc123", None);
        assert_eq!(encoded, "42:abc123");

        let parsed = parse_idempotent_outcome(&encoded).expect("should parse");
        assert_eq!(parsed.revision, 42);
        assert_eq!(parsed.content_hash, "abc123");
        assert_eq!(parsed.logical_digest, None);
    }

    #[test]
    fn a_logical_digest_rides_the_value_and_an_old_value_reads_as_none() {
        let encoded = encode_idempotent_value(42, "abc123", Some("d1g3st"));
        assert_eq!(encoded, "42:abc123:d1g3st");

        let parsed = parse_idempotent_outcome(&encoded).expect("should parse");
        assert_eq!(parsed.content_hash, "abc123");
        assert_eq!(parsed.logical_digest.as_deref(), Some("d1g3st"));

        let legacy = parse_idempotent_outcome("7:beef").expect("should parse");
        assert_eq!(legacy.revision, 7);
        assert_eq!(legacy.content_hash, "beef");
        assert_eq!(
            legacy.logical_digest, None,
            "a value written before the digest existed must not invent one"
        );
    }

    #[test]
    fn parse_idempotent_outcome_rejects_malformed_value() {
        let err = parse_idempotent_outcome("no-colon").expect_err("should fail");
        assert!(err.to_string().contains("malformed"));
    }

    #[test]
    fn parse_idempotent_outcome_rejects_non_numeric_revision() {
        let err = parse_idempotent_outcome("abc:hash").expect_err("should fail");
        assert!(err.to_string().contains("invalid idempotency revision"));
    }

    #[test]
    fn parse_revision_accepts_valid_numbers() {
        assert_eq!(parse_revision("0").expect("should parse"), 0);
        assert_eq!(parse_revision("123").expect("should parse"), 123);
    }

    #[test]
    fn parse_revision_rejects_invalid_input() {
        let err = parse_revision("abc").expect_err("should fail");
        assert!(err.to_string().contains("invalid revision value"));
    }
}
