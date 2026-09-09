//! Hash the complete selected content before the transport slices it into pages.
//! Protobuf map iteration is unspecified, so metadata is framed in sorted order.
use std::collections::BTreeMap;

use kmp_proto::v1beta1::{MemoryRelation, RelateResponse};
use prost::Message;
use sha2::{Digest, Sha256};

pub(super) struct ReadSelectionFingerprint(Sha256);

impl ReadSelectionFingerprint {
    pub(super) fn trace(summary: &str, path: &[MemoryRelation], found: bool) -> String {
        let mut digest = Self(Sha256::new());
        digest.bytes(b"kmp.trace.selection.v1");
        digest.bytes(summary.as_bytes());
        digest.bytes(&[u8::from(found)]);
        digest.messages("trace", path);
        digest.finish()
    }

    pub(super) fn relate(response: &RelateResponse) -> String {
        let mut digest = Self(Sha256::new());
        digest.bytes(b"kmp.relate.selection.v1");
        digest.bytes(response.summary.as_bytes());
        digest.bytes(b"facts");
        digest.bytes(&(response.facts.len() as u64).to_le_bytes());
        for fact in &response.facts {
            let mut canonical = fact.clone();
            canonical.metadata.clear();
            digest.message(&canonical);
            digest.metadata(&fact.metadata);
        }
        digest.messages("declared", &response.declared);
        digest.messages("coordinate", &response.coordinate);
        digest.messages("tensions", &response.tensions);
        digest.messages("proposed", &response.proposed);
        if let Some(proof) = &response.proof {
            let mut canonical = proof.clone();
            for item in &mut canonical.evidence {
                item.metadata.clear();
            }
            digest.bytes(b"proof");
            digest.message(&canonical);
            for item in &proof.evidence {
                digest.metadata(&item.metadata);
            }
        }
        digest.bytes(b"warnings");
        for warning in &response.warnings {
            if !warning.starts_with("relate page cursor ")
                && warning != "relate response paginated; use page.next_cursor to continue"
            {
                digest.bytes(warning.as_bytes());
            }
        }
        digest.finish()
    }

    fn messages<M: Message>(&mut self, name: &str, messages: &[M]) {
        self.bytes(name.as_bytes());
        self.bytes(&(messages.len() as u64).to_le_bytes());
        for message in messages {
            self.message(message);
        }
    }

    fn message(&mut self, message: &impl Message) {
        self.bytes(&message.encode_to_vec());
    }

    fn metadata(&mut self, values: &std::collections::HashMap<String, String>) {
        let sorted: BTreeMap<_, _> = values.iter().collect();
        self.bytes(&serde_json::to_vec(&sorted).expect("string metadata serializes"));
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    fn finish(self) -> String {
        format!("{:x}", self.0.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::{MemoryEvidence, Proof, RelatedFact};

    #[test]
    fn metadata_order_is_irrelevant_but_changed_metadata_invalidates_selection() {
        let pairs: Vec<_> = (0..12)
            .map(|i| (format!("key-{i}"), format!("value-{i}")))
            .collect();
        let mut response = RelateResponse {
            facts: vec![RelatedFact {
                r#ref: "source:a".to_string(),
                metadata: pairs.iter().cloned().collect(),
                ..Default::default()
            }],
            proof: Some(Proof {
                evidence: vec![MemoryEvidence {
                    metadata: pairs.iter().cloned().collect(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let expected = ReadSelectionFingerprint::relate(&response);
        response.facts[0].metadata = pairs.iter().rev().cloned().collect();
        response.proof.as_mut().expect("proof").evidence[0].metadata =
            pairs.iter().rev().cloned().collect();
        assert_eq!(ReadSelectionFingerprint::relate(&response), expected);
        response.facts[0]
            .metadata
            .insert("key-11".to_string(), "new value".to_string());
        assert_ne!(ReadSelectionFingerprint::relate(&response), expected);
    }
}
