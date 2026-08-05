use serde::{Deserialize, Serialize};

/// What became of a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedMemoryView {
    /// Echoed back unchanged.
    pub about: String,
    /// The kernel's identity for this record, derived from the idempotency
    /// key — which is why a retried record answers with the same one.
    pub memory_id: String,
    pub accepted_entries: usize,
    pub accepted_evidence: usize,
    /// Whether a recall issued now would already see this record. False means
    /// the write is committed but a projection still has to catch up.
    pub read_after_write_ready: bool,
    /// What the kernel accepted with reservations. A consumer that drops
    /// these silently is discarding the only account of what was bent.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_view_survives_the_wire() {
        let view = RecordedMemoryView {
            about: "project:checkout".to_string(),
            memory_id: "memory:record:1".to_string(),
            accepted_entries: 1,
            accepted_evidence: 2,
            read_after_write_ready: true,
            warnings: vec!["one coordinate had no occurred_at".to_string()],
        };
        let bytes = serde_json::to_vec(&view).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<RecordedMemoryView>(&bytes).expect("deserializes"),
            view
        );
    }
}
