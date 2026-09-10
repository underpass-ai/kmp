use serde_json::Value;
use sha2::{Digest, Sha256};

/// A canonical text definition found in the input, including its exact location.
pub(super) struct SourceText {
    pub reference: String,
    pub sha256: String,
    pub text: String,
    pub packet: usize,
    pub pointer: String,
}

impl SourceText {
    pub fn collect(packets: &Value) -> Vec<Self> {
        let mut sources = Vec::new();
        for (packet, body) in packets.as_array().into_iter().flatten().enumerate() {
            let mut add = |record: &Value, key: &str, pointer: String| {
                if let (Some(reference), Some(text)) =
                    (record[key].as_str(), record["text"].as_str())
                {
                    sources.push(Self {
                        reference: reference.into(),
                        sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
                        text: text.into(),
                        packet,
                        pointer,
                    });
                }
            };
            if let Some(object) = body.get("object") {
                add(object, "ref", "/object/text".into());
            }
            for (path, key) in [
                ("/entries", "ref"),
                ("/proof/entries", "ref"),
                ("/facts", "ref"),
                ("/evidence", "id"),
                ("/proof/evidence", "id"),
            ] {
                for (index, record) in body
                    .pointer(path)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    add(record, key, format!("{path}/{index}/text"));
                }
            }
        }
        sources
    }
}
