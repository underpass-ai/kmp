//! `size_batch` against `read_batch`: same slots, and a record measured
//! rather than a body.

use super::*;
use crate::adapter::engine::{Engine, sqlite::SqliteEngine};
use crate::adapter::serdes::encode;

fn detail(node_id: &str, body: &str) -> NodeDetailProjection {
    NodeDetailProjection {
        node_id: node_id.into(),
        detail: body.into(),
        content_hash: format!("hash:{node_id}"),
        revision: 9,
    }
}

#[test]
fn sizes_follow_the_requested_slots_and_measure_the_stored_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = SqliteEngine::open_file(&dir.path().join("kernel.sqlite3")).expect("engine opens");

    let stored = [
        detail("a", "Canónica π — la evidencia completa"),
        detail("b", ""),
    ];
    let mut write = engine.begin_write().expect("write transaction");
    for projection in &stored {
        let record = DetailRecord::from(projection.clone());
        let bytes = encode("node detail", &record).expect("encode");
        write
            .insert(Table::Details, Key::Str(&projection.node_id), &bytes)
            .expect("insert");
    }
    Box::new(write).commit().expect("commit");

    let tx = engine.begin_read().expect("read transaction");
    let ids = [
        "a".to_string(),
        "absent".to_string(),
        "b".to_string(),
        "a".to_string(),
    ];
    let sizes = size_batch(tx.as_ref(), &ids).expect("sizes");
    let bodies = read_batch(tx.as_ref(), &ids).expect("bodies");

    assert_eq!(sizes.len(), ids.len());
    assert_eq!(bodies.len(), ids.len());
    assert_eq!(
        sizes.iter().map(Option::is_some).collect::<Vec<_>>(),
        bodies.iter().map(Option::is_some).collect::<Vec<_>>(),
        "an absent size and an absent body must mark the same slots"
    );
    assert_eq!(sizes[1], None, "an id with no detail has no size");
    assert_eq!(sizes[0], sizes[3], "a repeated id is measured every time");

    for (size, body) in sizes.iter().zip(&bodies) {
        let (Some(size), Some(body)) = (size, body) else {
            continue;
        };
        let record = encode("node detail", &DetailRecord::from(body.clone())).expect("re-encode");
        assert_eq!(
            *size,
            record.len() as u64,
            "the probe measures the stored record"
        );
        assert!(
            *size > body.detail.len() as u64,
            "the record carries an envelope around the canonical body, so it is larger"
        );
    }
}
