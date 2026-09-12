//! Admission must never fetch a deferred real SQLite Details record.
use super::*;
use crate::adapter::engine::{Str3Row, StrRow, U64Row};
use std::cell::RefCell;

struct RecordingRead<'a> {
    inner: &'a dyn ReadTx,
    details: RefCell<Vec<String>>,
}
impl ReadTx for RecordingRead<'_> {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        if let (Table::Details, Key::Str(id)) = (table, key) {
            self.details.borrow_mut().push(id.into());
            assert_ne!(
                id, "source",
                "deferred 64 MiB Details must never be fetched"
            );
        }
        self.inner.get(table, key)
    }
    fn value_len(&self, t: Table, k: Key<'_>) -> Result<Option<u64>, PortError> {
        self.inner.value_len(t, k)
    }
    fn scan_str(&self, t: Table) -> Result<Vec<StrRow>, PortError> {
        assert_ne!(t, Table::Details, "no bulk Details read");
        self.inner.scan_str(t)
    }
    fn scan_str3_by_first(&self, t: Table, f: &str) -> Result<Vec<Str3Row>, PortError> {
        self.inner.scan_str3_by_first(t, f)
    }
    fn scan_str3_page(
        &self,
        t: Table,
        f: &str,
        a: Option<(&str, &str)>,
        l: u32,
        r: Option<&str>,
    ) -> Result<Vec<Str3Row>, PortError> {
        self.inner.scan_str3_page(t, f, a, l, r)
    }
    fn scan_u64(&self, t: Table) -> Result<Vec<U64Row>, PortError> {
        self.inner.scan_u64(t)
    }
    fn last_u64(&self, t: Table) -> Result<Option<U64Row>, PortError> {
        self.inner.last_u64(t)
    }
    fn count(&self, t: Table) -> Result<u64, PortError> {
        self.inner.count(t)
    }
}

#[tokio::test]
async fn real_64_mib_body_is_described_but_never_fetched_under_8_mib() {
    const BODY_BYTES: u64 = 64 * 1024 * 1024;
    const BUDGET: u64 = 8 * 1024 * 1024;
    let dir = tempfile::tempdir().expect("fixture");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut mutations = changes("old");
    for mutation in &mut mutations {
        if let ProjectionMutation::UpsertNodeDetail(detail) = mutation
            && detail.node_id == "source"
        {
            detail.detail = "S".repeat(BODY_BYTES as usize);
        }
    }
    store
        .apply_mutations(mutations)
        .await
        .expect("persist real large body");
    let tx = store.begin_read().expect("snapshot");
    let exact_record = tx
        .value_len(Table::Details, Key::Str("source"))
        .expect("size")
        .expect("stored");
    assert!(exact_record > BODY_BYTES);
    let recording = RecordingRead {
        inner: tx.as_ref(),
        details: RefCell::default(),
    };
    let query = TraceSearchRequest {
        proof: true,
        about: "project:proof".into(),
        from: "a".into(),
        targets: ["b".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".into()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        limits: Default::default(),
        temporal: Default::default(),
        body: TraceBodyOptions {
            max_record_bytes: Some(BUDGET),
            ..Default::default()
        },
    };
    let proof = bounded_trace_search(&TraceSnapshot(&recording), &query)
        .expect("trace")
        .proof
        .expect("proof");
    let source = proof
        .objects
        .iter()
        .find(|o| o.node.node_id == "source")
        .expect("source");
    let descriptor = source.descriptor.as_ref().expect("descriptor");
    assert_eq!(descriptor.body_bytes, BODY_BYTES);
    assert_eq!(descriptor.record_bytes, exact_record);
    assert_eq!(source.body_state, TraceBodyState::DeferredBudget);
    assert!(source.body.is_none());
    assert_eq!(source.required_record_bytes(), Some(exact_record));
    assert!(
        proof
            .delivery
            .as_ref()
            .expect("delivery")
            .admitted_record_bytes
            <= BUDGET
    );
    assert_eq!(*recording.details.borrow(), ["a", "b"]);
}
