use serde_json::Value;
use sha2::{Digest, Sha256};

use super::verdict_key::field;

/// The state of a request as the key sees it: its JSON shape, with every
/// string replaced by the sha256 of its exact text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StateDigest([u8; 32]);

impl StateDigest {
    pub(crate) fn of(state: &Value) -> Self {
        let mut hasher = Sha256::new();
        digest_state(&mut hasher, state);
        Self(hasher.finalize().into())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn digest_state(hasher: &mut Sha256, value: &Value) {
    match value {
        Value::String(text) => {
            hasher.update(b"s");
            hasher.update(Sha256::digest(text.as_bytes()));
        }
        Value::Array(items) => {
            hasher.update(b"[");
            hasher.update((items.len() as u64).to_be_bytes());
            for item in items {
                digest_state(hasher, item);
            }
        }
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            hasher.update(b"{");
            hasher.update((keys.len() as u64).to_be_bytes());
            for key in keys {
                field(hasher, key.as_bytes());
                digest_state(hasher, &map[key]);
            }
        }
        other => field(hasher, other.to_string().as_bytes()),
    }
}
