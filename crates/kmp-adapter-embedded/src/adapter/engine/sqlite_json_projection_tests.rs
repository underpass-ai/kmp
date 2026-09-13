use super::{Ops, create_tables};
use crate::adapter::engine::{Key, Table};
use rusqlite::Connection;

#[test]
fn json_headers_return_only_selected_fields_independent_of_discarded_source_size() {
    let connection = Connection::open_in_memory().expect("fixture operation succeeds");
    create_tables(&connection).expect("fixture operation succeeds");
    let ops = Ops {
        connection: &connection,
    };
    assert_eq!(
        ops.project_str_json(Table::Nodes, "absent", &["node_id"])
            .expect("fixture operation succeeds"),
        None
    );
    for size in [1024, 1024 * 1024] {
        let value = serde_json::json!({"node_id":"n", "summary":"áλ".repeat(size), "labels":["evidence"],
            "properties":{"placeholder":"true", "canonical":"B".repeat(size)}});
        ops.insert(
            Table::Nodes,
            Key::Str("n"),
            &serde_json::to_vec(&value).expect("fixture operation succeeds"),
        )
        .expect("fixture operation succeeds");
        let header = ops
            .project_str_json(
                Table::Nodes,
                "n",
                &["node_id", "labels", "properties.placeholder", "missing"],
            )
            .expect("fixture operation succeeds")
            .expect("fixture operation succeeds");
        let actual: serde_json::Value =
            serde_json::from_slice(&header).expect("fixture operation succeeds");
        assert_eq!(
            actual,
            serde_json::json!({"node_id":"n", "labels":["evidence"], "properties.placeholder":"true", "missing":null})
        );
        assert!(
            header.len() < 128,
            "only the declared header crosses the engine seam"
        );
        println!(
            "discarded source scale {size}: {} header bytes returned",
            header.len()
        );
    }
    ops.insert(Table::Nodes, Key::Str("bad"), b"not-json")
        .expect("fixture operation succeeds");
    assert!(
        ops.project_str_json(Table::Nodes, "bad", &["node_id"])
            .is_err()
    );
    assert!(
        ops.project_str_json(Table::Relations, "n", &["node_id"])
            .is_err()
    );
}
