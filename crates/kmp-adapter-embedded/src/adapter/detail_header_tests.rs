//! Canonical body headers: the bytes they describe, when they refuse to
//! answer, and that they move with their detail or not at all.

use super::*;
use crate::adapter::engine::Engine;
use crate::adapter::engine::sqlite::SqliteEngine;
use crate::adapter::format_version::{StorageEngine, store_file_path_for};
use crate::adapter::projection_write::apply_mutations_in_transaction;
use crate::adapter::store::EmbeddedKernelStore;
use kmp_domain::{ContextUpdatedEvent, ProjectionMutation, ProjectionWriter};

/// Multibyte text plus every character JSON has to escape, so the stored
/// record cannot coincidentally equal the canonical body.
const AWKWARD: &str = "Canónica π \"entre comillas\" \\ barra\n\tsalto ✓ 記録";

fn detail(node_id: &str, body: &str, revision: u64, content_hash: &str) -> NodeDetailProjection {
    NodeDetailProjection {
        node_id: node_id.into(),
        detail: body.into(),
        content_hash: content_hash.into(),
        revision,
    }
}

fn engine() -> (tempfile::TempDir, SqliteEngine) {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = SqliteEngine::open_file(&dir.path().join("kernel.sqlite3")).expect("engine opens");
    (dir, engine)
}

fn write_details(engine: &SqliteEngine, details: &[NodeDetailProjection]) {
    let mut tx = engine.begin_write().expect("write transaction");
    let mutations = details
        .iter()
        .cloned()
        .map(ProjectionMutation::UpsertNodeDetail)
        .collect();
    apply_mutations_in_transaction(tx.as_mut(), mutations).expect("mutations apply");
    Box::new(tx).commit().expect("commit");
}

fn stored_record(engine: &SqliteEngine, node_id: &str) -> Vec<u8> {
    let tx = engine.begin_read().expect("read transaction");
    tx.get(Table::Details, Key::Str(node_id))
        .expect("read")
        .expect("the detail was written")
}

fn header_of(engine: &SqliteEngine, node_id: &str) -> DetailHeaderRecord {
    let tx = engine.begin_read().expect("read transaction");
    read_batch(tx.as_ref(), &[node_id.to_string()])
        .expect("headers")
        .remove(0)
        .expect("the body exists")
}

#[test]
fn a_header_describes_the_exact_serialized_record_and_the_canonical_body() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", AWKWARD, 7, "public:token")]);

    let record = stored_record(&engine, "a");
    let header = header_of(&engine, "a");

    assert_eq!(header.node_id, "a");
    assert_eq!(header.revision, 7);
    assert_eq!(
        header.content_hash, "public:token",
        "the public token is copied literally, never recomputed"
    );
    assert_eq!(header.record_bytes, record.len() as u64);
    assert_eq!(
        header.record_digest,
        format!("sha256:{:x}", Sha256::digest(&record)),
        "the digest covers the exact bytes the store holds"
    );
    assert_eq!(
        header.body_bytes,
        AWKWARD.len() as u64,
        "body bytes are canonical UTF-8, not the escaped form in the record"
    );
    assert!(
        header.record_bytes > header.body_bytes,
        "escaping and the record envelope only add: {} against {}",
        header.record_bytes,
        header.body_bytes
    );
    assert!(header.record_digest.starts_with("sha256:"));
    assert_eq!(
        header.record_digest,
        header.record_digest.to_lowercase(),
        "hex stays lowercase"
    );
}

#[test]
fn changed_text_under_the_same_public_version_and_hash_still_changes_the_digest() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", "primera versión", 4, "public:same")]);
    let before = header_of(&engine, "a");

    write_details(
        &engine,
        &[detail("a", "segunda versión ✓", 4, "public:same")],
    );
    let after = header_of(&engine, "a");

    assert_eq!(after.revision, before.revision);
    assert_eq!(after.content_hash, before.content_hash);
    assert_ne!(
        after.record_digest, before.record_digest,
        "a public token that did not move must not hide changed bytes"
    );
    assert_ne!(after.body_bytes, before.body_bytes);
    assert_eq!(
        after.record_bytes,
        stored_record(&engine, "a").len() as u64,
        "the update replaced the header along with its detail"
    );
}

#[test]
fn an_uncommitted_batch_leaves_neither_the_detail_nor_its_header() {
    let (_dir, engine) = engine();
    let mut tx = engine.begin_write().expect("write transaction");
    apply_mutations_in_transaction(
        tx.as_mut(),
        vec![ProjectionMutation::UpsertNodeDetail(detail(
            "a", AWKWARD, 1, "public:x",
        ))],
    )
    .expect("mutations apply");
    drop(tx);

    let tx = engine.begin_read().expect("read transaction");
    assert_eq!(
        tx.value_len(Table::Details, Key::Str("a")).expect("probe"),
        None
    );
    assert_eq!(
        tx.get(Table::DetailHeaders, Key::Str("a")).expect("read"),
        None
    );
    assert_eq!(
        read_batch(tx.as_ref(), &["a".to_string()]).expect("headers"),
        vec![None]
    );
}

#[test]
fn only_an_absent_body_reads_as_none_and_slots_follow_the_request() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", "cuerpo", 1, "public:a")]);

    let tx = engine.begin_read().expect("read transaction");
    let ids = [
        "a".to_string(),
        "never-written".to_string(),
        "a".to_string(),
    ];
    let headers = read_batch(tx.as_ref(), &ids).expect("headers");

    assert_eq!(headers.len(), 3);
    assert!(headers[0].is_some());
    assert_eq!(headers[1], None, "no detail row is a genuinely absent body");
    assert_eq!(headers[0], headers[2], "a repeated id answers the same");
}

#[test]
fn a_detail_whose_header_is_absent_demands_a_rebuild_instead_of_reporting_absence() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", AWKWARD, 1, "public:x")]);

    // The shape of a store written before this table existed.
    let mut tx = engine.begin_write().expect("write transaction");
    tx.remove(Table::DetailHeaders, Key::Str("a"))
        .expect("drop the header");
    Box::new(tx).commit().expect("commit");

    let tx = engine.begin_read().expect("read transaction");
    let error = read_batch(tx.as_ref(), &["a".to_string()])
        .expect_err("a stored body without its header is inconsistent");
    let message = error.to_string();
    assert!(
        message.contains("`a`") && message.contains("rebuild"),
        "the refusal must name the body and the remedy, got: {message}"
    );
}

#[test]
fn a_header_disagreeing_with_the_stored_record_length_is_refused() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", AWKWARD, 1, "public:x")]);
    let mut header = header_of(&engine, "a");
    header.record_bytes += 1;

    let mut tx = engine.begin_write().expect("write transaction");
    write(tx.as_mut(), &header).expect("overwrite the header");
    Box::new(tx).commit().expect("commit");

    let tx = engine.begin_read().expect("read transaction");
    let error = read_batch(tx.as_ref(), &["a".to_string()])
        .expect_err("a header that does not match its record is inconsistent");
    let message = error.to_string();
    assert!(
        message.contains("`a`") && message.contains("rebuild"),
        "got: {message}"
    );
}

#[test]
fn a_malformed_header_fails_to_decode_rather_than_answering() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", AWKWARD, 1, "public:x")]);

    let mut tx = engine.begin_write().expect("write transaction");
    tx.insert(Table::DetailHeaders, Key::Str("a"), b"{not a header}")
        .expect("corrupt the header");
    Box::new(tx).commit().expect("commit");

    let tx = engine.begin_read().expect("read transaction");
    let error = read_batch(tx.as_ref(), &["a".to_string()]).expect_err("malformed header");
    assert!(
        error.to_string().contains("node detail header"),
        "got: {error}"
    );
}

#[test]
fn a_pinned_read_sees_one_state_of_the_detail_and_its_header() {
    let (_dir, engine) = engine();
    write_details(&engine, &[detail("a", "primera", 1, "public:x")]);
    let first = header_of(&engine, "a");

    let pinned = engine.begin_read().expect("read transaction");
    assert_eq!(
        read_batch(pinned.as_ref(), &["a".to_string()]).expect("headers")[0],
        Some(first.clone())
    );

    write_details(
        &engine,
        &[detail("a", "segunda, mucho más larga", 2, "public:y")],
    );

    assert_eq!(
        read_batch(pinned.as_ref(), &["a".to_string()]).expect("headers")[0],
        Some(first),
        "the pinned read must not mix a new header with an old record"
    );
    drop(pinned);

    let after = header_of(&engine, "a");
    assert_eq!(after.revision, 2);
    assert_eq!(after.record_bytes, stored_record(&engine, "a").len() as u64);
}

fn no_events(_event: &ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> {
    Ok(Vec::new())
}

/// A rebuild clears both tables in the transaction that rewrites them, so a
/// header can never outlive the detail it describes. The rebuilding half runs
/// through `apply_mutations_in_transaction`, the same seam the write tests
/// above exercise.
#[tokio::test]
async fn a_rebuild_clears_the_headers_with_their_details() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store opens");
    store
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(detail(
            "a", AWKWARD, 1, "public:x",
        ))])
        .await
        .expect("write");

    let file = store_file_path_for(dir.path(), StorageEngine::Sqlite);
    let observer = SqliteEngine::open_file(&file).expect("second handle");
    let before = observer.begin_read().expect("read transaction");
    assert_eq!(before.count(Table::Details).expect("count"), 1);
    assert_eq!(before.count(Table::DetailHeaders).expect("count"), 1);
    drop(before);

    store
        .rebuild_projections(no_events)
        .await
        .expect("rebuild from an empty event log");

    let after = observer.begin_read().expect("read transaction");
    assert_eq!(after.count(Table::Details).expect("count"), 0);
    assert_eq!(
        after.count(Table::DetailHeaders).expect("count"),
        0,
        "a cleared detail must not leave its header behind"
    );
}
