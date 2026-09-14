use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kmp_domain::{
    MemoryDimensionIdentity, NodeProjection, ProjectionMutation, ProjectionWriter,
    RelationExplanation, RelationSemanticClass,
};

use super::*;
use crate::adapter::engine::{Key, LinkedJsonRow, LinkedJsonScan, Str3Row, StrRow, U64Row};
use crate::adapter::store::EmbeddedKernelStore;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ScanCount {
    calls: usize,
    rows: usize,
    bytes: usize,
}

struct RecordingRead<'a> {
    inner: &'a dyn ReadTx,
    scans: RefCell<BTreeMap<String, ScanCount>>,
}

impl RecordingRead<'_> {
    fn count(&self, source: &str) -> ScanCount {
        self.scans.borrow().get(source).copied().unwrap_or_default()
    }

    fn totals(&self) -> ScanCount {
        self.scans
            .borrow()
            .values()
            .copied()
            .fold(ScanCount::default(), |total, count| ScanCount {
                calls: total.calls + count.calls,
                rows: total.rows + count.rows,
                bytes: total.bytes + count.bytes,
            })
    }
}

impl ReadTx for RecordingRead<'_> {
    fn scan_linked_json(
        &self,
        request: &LinkedJsonScan<'_>,
    ) -> Result<Vec<LinkedJsonRow>, PortError> {
        self.inner.scan_linked_json(request)
    }

    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        self.inner.get(table, key)
    }

    fn project_str_json(
        &self,
        table: Table,
        key: &str,
        fields: &[&str],
    ) -> Result<Option<Vec<u8>>, PortError> {
        self.inner.project_str_json(table, key, fields)
    }

    fn value_len(&self, table: Table, key: Key<'_>) -> Result<Option<u64>, PortError> {
        self.inner.value_len(table, key)
    }

    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        self.inner.scan_str(table)
    }

    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        let rows = self.inner.scan_str3_by_first(table, first)?;
        if table == Table::Relations {
            let count = self.scans.borrow().get(first).copied().unwrap_or_default();
            self.scans.borrow_mut().insert(
                first.to_string(),
                ScanCount {
                    calls: count.calls + 1,
                    rows: count.rows + rows.len(),
                    bytes: count.bytes + rows.iter().map(|(_, value)| value.len()).sum::<usize>(),
                },
            );
        }
        Ok(rows)
    }

    fn scan_str3_page(
        &self,
        table: Table,
        first: &str,
        after: Option<(&str, &str)>,
        limit: u32,
        relation_type: Option<&str>,
    ) -> Result<Vec<Str3Row>, PortError> {
        self.inner
            .scan_str3_page(table, first, after, limit, relation_type)
    }

    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        self.inner.scan_u64(table)
    }

    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        self.inner.last_u64(table)
    }

    fn count(&self, table: Table) -> Result<u64, PortError> {
        self.inner.count(table)
    }
}

fn node(id: &str, kind: &str, status: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: id.into(),
        summary: format!("summary for {id}"),
        status: status.into(),
        labels: Vec::new(),
        properties: BTreeMap::new(),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, relation: &str, explanation: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: relation.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_rationale(format!("why {explanation}"))
            .with_evidence(format!("evidence {explanation}")),
    }))
}

fn legacy_read(
    tx: &dyn ReadTx,
    request: &NeighborhoodRequest,
) -> Result<(BTreeSet<String>, Vec<NodeRelationProjection>), PortError> {
    let root = request.root_node_id();
    let mut visited = BTreeSet::from([root.to_string()]);
    let mut reachable = BTreeSet::new();
    let mut frontier = VecDeque::from([(root.to_string(), 0u32)]);
    while let Some((source, hops)) = frontier.pop_front() {
        if hops == request.depth() {
            continue;
        }
        for ((_, target, _), _) in tx.scan_str3_by_first(Table::Relations, &source)? {
            if request.admits(&target) && visited.insert(target.clone()) {
                reachable.insert(target.clone());
                frontier.push_back((target, hops + 1));
            }
        }
    }
    reachable.remove(root);
    let mut relations = Vec::new();
    if !reachable.is_empty() {
        let mut selected = reachable.clone();
        selected.insert(root.to_string());
        for source in &selected {
            for ((source_node_id, target_node_id, relation_type), raw) in
                tx.scan_str3_by_first(Table::Relations, source)?
            {
                let explanation = decode_explanation(&raw)?;
                if selected.contains(&target_node_id) {
                    relations.push(NodeRelationProjection {
                        source_node_id,
                        target_node_id,
                        relation_type,
                        explanation,
                    });
                }
            }
        }
    }
    Ok((reachable, relations))
}

fn encoded_relation_bytes(tx: &dyn ReadTx, relations: &[NodeRelationProjection]) -> usize {
    relations
        .iter()
        .map(|relation| {
            tx.get(
                Table::Relations,
                Key::Str3(
                    &relation.source_node_id,
                    &relation.target_node_id,
                    &relation.relation_type,
                ),
            )
            .expect("relation read")
            .expect("relation exists")
            .len()
        })
        .sum()
}

async fn parity_store(
    path: &std::path::Path,
) -> Result<(EmbeddedKernelStore, NeighborhoodRequest), PortError> {
    let about = "project:adjacency-test";
    let selected = MemoryDimensionIdentity::new(about, "task", "selected")
        .expect("selected identity")
        .node_id();
    let discarded = MemoryDimensionIdentity::new(about, "topic", "discarded")
        .expect("discarded identity")
        .node_id();
    let store = EmbeddedKernelStore::open(path)?;
    store
        .apply_mutations(vec![
            node(about, "memory_anchor", "ACTIVE"),
            node(&selected, "memory_dimension", "ACTIVE"),
            node(&discarded, "memory_dimension", "ACTIVE"),
            node("entry:a", "memory_entry", "SUPERSEDED"),
            node("entry:b", "memory_entry", "EXPIRED"),
            edge(about, &selected, "has_dimension", "root-selected"),
            edge(about, &discarded, "has_dimension", "root-discarded"),
            edge(&selected, "entry:a", "contains_entry", "selected-a"),
            edge(&selected, "entry:a", "mentions", "selected-a-duplicate"),
            edge("entry:a", "entry:b", "follows", "a-b"),
            edge("entry:a", &discarded, "mentions", "a-discarded"),
            edge("entry:b", "entry:a", "uses_background", "b-a"),
        ])
        .await?;
    Ok((
        store,
        NeighborhoodRequest::new(about, 3).with_scopes([selected]),
    ))
}

#[tokio::test]
async fn one_scan_per_selected_source_matches_the_independent_legacy_result() {
    let dir = tempfile::tempdir().expect("dir");
    let (store, request) = parity_store(dir.path()).await.expect("fixture");
    let tx = store.begin_read().expect("snapshot");
    let recording = RecordingRead {
        inner: tx.as_ref(),
        scans: RefCell::default(),
    };
    let actual = OutwardNeighborhoodRead::new(&recording, &request)
        .catalogue()
        .expect("new read");
    let legacy_recording = RecordingRead {
        inner: tx.as_ref(),
        scans: RefCell::default(),
    };
    let expected = legacy_read(&legacy_recording, &request).expect("legacy oracle");
    assert_eq!(actual, expected);

    for source in [
        request.root_node_id(),
        request.scopes().first().expect("scope"),
        "entry:a",
        "entry:b",
    ] {
        assert_eq!(recording.count(source).calls, 1, "source {source}");
    }
    assert_eq!(recording.count("entry:a").rows, 2);
    assert!(recording.count("entry:a").bytes > 0);
    let decoded_bytes = encoded_relation_bytes(tx.as_ref(), &actual.1);
    eprintln!(
        "graph-read-counters candidate={:?} legacy={:?} candidate_decodes={} \
         candidate_decode_bytes={} legacy_decodes={} legacy_decode_bytes={}",
        recording.totals(),
        legacy_recording.totals(),
        actual.1.len(),
        decoded_bytes,
        recording.totals().rows,
        recording.totals().bytes,
    );
}

#[tokio::test]
async fn zero_depth_keeps_the_empty_result_without_reading_adjacency() {
    let dir = tempfile::tempdir().expect("dir");
    let (store, request) = parity_store(dir.path()).await.expect("fixture");
    let request = NeighborhoodRequest::new(request.root_node_id(), 0);
    let mut write = store.begin_write().expect("write");
    write
        .insert(
            Table::Relations,
            Key::Str3(request.root_node_id(), request.root_node_id(), "self"),
            b"not relation JSON",
        )
        .expect("malformed depth-zero self-loop");
    write.commit().expect("commit");
    let tx = store.begin_read().expect("snapshot");
    let recording = RecordingRead {
        inner: tx.as_ref(),
        scans: RefCell::default(),
    };

    let result = OutwardNeighborhoodRead::new(&recording, &request)
        .catalogue()
        .expect("zero-depth read");
    let expected = legacy_read(tx.as_ref(), &request).expect("legacy oracle");

    assert_eq!(result, expected);
    assert_eq!(result, (BTreeSet::new(), Vec::new()));
    assert_eq!(recording.totals(), ScanCount::default());
}

#[tokio::test]
async fn isolated_root_does_not_decode_a_malformed_self_loop() {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut write = store.begin_write().expect("write");
    write
        .insert(
            Table::Relations,
            Key::Str3("isolated", "isolated", "self"),
            b"not relation JSON",
        )
        .expect("malformed self-loop");
    write.commit().expect("commit");
    let request = NeighborhoodRequest::new("isolated", 1);
    let tx = store.begin_read().expect("snapshot");
    let recording = RecordingRead {
        inner: tx.as_ref(),
        scans: RefCell::default(),
    };

    let actual = OutwardNeighborhoodRead::new(&recording, &request)
        .catalogue()
        .expect("isolated root ignores relations");
    let expected = legacy_read(tx.as_ref(), &request).expect("legacy oracle");

    assert_eq!(actual, expected);
    assert_eq!(actual, (BTreeSet::new(), Vec::new()));
    assert_eq!(recording.count("isolated").calls, 1);
}

#[tokio::test]
async fn discarded_explanations_are_not_decoded_but_retained_ones_remain_validated() {
    let dir = tempfile::tempdir().expect("dir");
    let (store, request) = parity_store(dir.path()).await.expect("fixture");
    let discarded = MemoryDimensionIdentity::new(request.root_node_id(), "topic", "discarded")
        .expect("identity")
        .node_id();
    let mut write = store.begin_write().expect("write");
    write
        .insert(
            Table::Relations,
            Key::Str3("entry:a", &discarded, "mentions"),
            b"not relation JSON",
        )
        .expect("corrupt discarded explanation");
    write.commit().expect("commit");

    let tx = store.begin_read().expect("snapshot");
    let result = OutwardNeighborhoodRead::new(tx.as_ref(), &request)
        .catalogue()
        .expect("discarded explanation is never decoded");
    assert!(result.1.iter().all(|edge| edge.target_node_id != discarded));

    drop(tx);
    let mut write = store.begin_write().expect("write");
    write
        .insert(
            Table::Relations,
            Key::Str3("entry:a", "entry:b", "follows"),
            b"not relation JSON",
        )
        .expect("corrupt retained explanation");
    write.commit().expect("commit");
    let tx = store.begin_read().expect("snapshot");
    let error = OutwardNeighborhoodRead::new(tx.as_ref(), &request)
        .catalogue()
        .expect_err("retained explanation must be decoded and validated");
    assert!(error.to_string().contains("relation explanation"));
}

#[tokio::test]
async fn one_operation_keeps_its_explanations_on_the_pinned_snapshot() {
    let dir = tempfile::tempdir().expect("dir");
    let (reader, request) = parity_store(dir.path()).await.expect("fixture");
    let writer = EmbeddedKernelStore::open(dir.path()).expect("writer");
    let tx = reader.begin_read().expect("snapshot");
    let selected = request.scopes().first().expect("scope");
    tx.get(
        Table::Relations,
        Key::Str3(request.root_node_id(), selected, "has_dimension"),
    )
    .expect("pin snapshot")
    .expect("old relation");
    writer
        .apply_mutations(vec![edge(
            request.root_node_id(),
            selected,
            "has_dimension",
            "new commit",
        )])
        .await
        .expect("concurrent commit");

    let old = OutwardNeighborhoodRead::new(tx.as_ref(), &request)
        .catalogue()
        .expect("old snapshot");
    let pinned = old
        .1
        .iter()
        .find(|edge| {
            edge.source_node_id == request.root_node_id() && edge.target_node_id == *selected
        })
        .expect("root relation");
    assert_eq!(
        pinned.explanation.evidence(),
        Some("evidence root-selected")
    );
    drop(tx);

    let tx = reader.begin_read().expect("fresh snapshot");
    let fresh = OutwardNeighborhoodRead::new(tx.as_ref(), &request)
        .catalogue()
        .expect("fresh read");
    let updated = fresh
        .1
        .iter()
        .find(|edge| {
            edge.source_node_id == request.root_node_id() && edge.target_node_id == *selected
        })
        .expect("root relation");
    assert_eq!(updated.explanation.evidence(), Some("evidence new commit"));
}
