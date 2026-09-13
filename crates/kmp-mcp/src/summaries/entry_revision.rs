use kmp_domain::SearchSummary;
use serde_json::Value;

/// One write of one memory entry, as the store's event log recorded it.
///
/// The reader keeps every revision rather than only the last, because the
/// earlier ones are where "the summary predates this text" is written: a
/// later ingest that replaced the text and left the metadata alone leaves no
/// other trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryRevision {
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) summary: Option<String>,
    pub(crate) summary_by: Option<String>,
}

impl EntryRevision {
    /// The entry as one change of one event describes it. Absent fields read
    /// as the store reads them elsewhere: no text, the default kind, no
    /// summary.
    pub(crate) fn from_payload(payload: &Value) -> Self {
        let metadata = &payload["metadata"];
        Self {
            kind: payload["kind"].as_str().unwrap_or("entry").to_string(),
            text: payload["text"].as_str().unwrap_or_default().to_string(),
            summary: metadata[SearchSummary::METADATA_KEY]
                .as_str()
                .map(str::to_string),
            summary_by: metadata[SearchSummary::SUMMARY_WRITER_METADATA_KEY]
                .as_str()
                .map(str::to_string),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_without_metadata_reads_as_an_entry_without_a_summary() {
        let revision =
            EntryRevision::from_payload(&serde_json::json!({"text": "The valve froze."}));

        assert_eq!(revision.kind, "entry");
        assert_eq!(revision.text, "The valve froze.");
        assert_eq!(revision.summary, None);
        assert_eq!(revision.summary_by, None);
    }

    #[test]
    fn the_writer_of_a_summary_travels_with_it() {
        let revision = EntryRevision::from_payload(&serde_json::json!({
            "kind": "decision",
            "text": "Se adoptó Valkey 7.2.",
            "metadata": {"summary_en": "Valkey 7.2 was adopted.", "summary_en_by": "agent:backfill"}
        }));

        assert_eq!(revision.kind, "decision");
        assert_eq!(revision.summary.as_deref(), Some("Valkey 7.2 was adopted."));
        assert_eq!(revision.summary_by.as_deref(), Some("agent:backfill"));
    }
}
