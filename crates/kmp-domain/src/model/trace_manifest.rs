//! The identity of one selected proof table, computed before any body is read.
//!
//! Two responses may only be joined by ref when they describe the same
//! selection. That is what this identity is for, and it is why it covers the
//! selected graph, the support declarations, the clocks, the source metadata
//! and every body descriptor — including the ones whose bodies were withheld
//! and the refs that are missing — rather than only the tokens of the bodies
//! that happened to be delivered.
//!
//! It deliberately excludes the byte ceiling, the requested ref subset, the
//! response page position and every card. Asking for a different slice of the
//! same selection is a new expansion of the same manifest, not a different
//! manifest; and a reader authoring a card must not invalidate another
//! reader's expansion of canonical text.

use sha2::{Digest, Sha256};

use crate::{NodeBodyDescriptor, NodeProjection, NodeRelationProjection, TemporalCoordinate};

pub struct TraceManifestDigest(Sha256);

impl TraceManifestDigest {
    pub fn new(tag: &str) -> Self {
        let mut digest = Self(Sha256::new());
        digest.bytes(tag.as_bytes());
        digest
    }

    /// Length-framed, so no two different sequences of fields can collide by
    /// running into each other.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    pub fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub fn number(&mut self, value: u64) {
        self.0.update(value.to_le_bytes());
    }

    pub fn flag(&mut self, value: bool) {
        self.0.update([u8::from(value)]);
    }

    pub fn texts<'a>(&mut self, values: impl IntoIterator<Item = &'a str>) {
        let values: Vec<&str> = values.into_iter().collect();
        self.number(values.len() as u64);
        for value in values {
            self.text(value);
        }
    }

    pub fn node(&mut self, node: &NodeProjection) {
        self.text(&node.node_id);
        self.text(&node.node_kind);
        self.text(&node.title);
        self.text(&node.summary);
        self.text(&node.status);
        self.texts(node.labels.iter().map(String::as_str));
        self.number(node.properties.len() as u64);
        for (key, value) in &node.properties {
            self.text(key);
            self.text(value);
        }
        match &node.provenance {
            Some(provenance) => {
                self.flag(true);
                self.text(provenance.source_kind().as_str());
                self.text(provenance.source_agent().unwrap_or_default());
                self.text(provenance.observed_at().unwrap_or_default());
            }
            None => self.flag(false),
        }
    }

    pub fn descriptor(&mut self, descriptor: Option<&NodeBodyDescriptor>) {
        match descriptor {
            Some(descriptor) => {
                self.flag(true);
                self.number(descriptor.revision);
                self.text(&descriptor.content_hash);
                self.number(descriptor.record_bytes);
                self.number(descriptor.body_bytes);
                self.text(&descriptor.record_digest);
            }
            // A ref whose body is absent is part of the selection's identity:
            // a later write that supplies it is a different selection.
            None => self.flag(false),
        }
    }

    pub fn relation(&mut self, edge: &NodeRelationProjection) {
        self.text(&edge.source_node_id);
        self.text(&edge.target_node_id);
        self.text(&edge.relation_type);
        let explanation = &edge.explanation;
        self.text(explanation.semantic_class().as_str());
        self.text(explanation.rationale().unwrap_or_default());
        self.text(explanation.evidence().unwrap_or_default());
        self.text(explanation.occurred_at().unwrap_or_default());
        self.text(explanation.observed_at().unwrap_or_default());
        self.text(explanation.ingested_at().unwrap_or_default());
        self.text(explanation.valid_from().unwrap_or_default());
        self.text(explanation.valid_until().unwrap_or_default());
    }

    pub fn coordinates(&mut self, coordinates: &[TemporalCoordinate]) {
        self.number(coordinates.len() as u64);
        for coordinate in coordinates {
            self.text(coordinate.dimension());
            self.text(coordinate.scope_id());
            self.text(coordinate.occurred_at().unwrap_or_default());
            self.text(coordinate.observed_at().unwrap_or_default());
            self.text(coordinate.ingested_at().unwrap_or_default());
            self.text(coordinate.valid_from().unwrap_or_default());
            self.text(coordinate.valid_until().unwrap_or_default());
            self.number(u64::from(coordinate.sequence().unwrap_or_default()));
            self.number(u64::from(coordinate.rank().unwrap_or_default()));
        }
    }

    pub fn finish(self) -> String {
        format!("{:x}", self.0.finalize())
    }
}
