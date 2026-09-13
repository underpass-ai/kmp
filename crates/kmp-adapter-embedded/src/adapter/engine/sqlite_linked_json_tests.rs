use super::{Ops, create_tables};
use crate::adapter::engine::{Key, LinkedJsonScan, Table, sqlite_linked_json};
use rusqlite::Connection;

#[test]
fn late_joined_pages_seek_within_one_root_without_revisiting_its_prior_links() {
    let connection = Connection::open_in_memory().expect("database");
    create_tables(&connection).expect("tables");
    let ops = Ops {
        connection: &connection,
    };
    for source in ["a", "z"] {
        ops.insert(Table::Anchors, Key::Str(source), &[])
            .expect("anchor");
    }
    for i in 0..10_000 {
        ops.insert(
            Table::Relations,
            Key::Str3("a", &format!("n{i:05}"), "selected"),
            &[],
        )
        .expect("link");
    }
    // The lower-bound optimization must retain an empty key on a later root.
    ops.insert(Table::Relations, Key::Str3("z", "", "selected"), &[])
        .expect("empty target");
    let request = LinkedJsonScan {
        roots: Table::Anchors,
        links: Table::Relations,
        records: Table::Nodes,
        relation: "selected",
        fields: &["id"],
        after: Some(("a", "n09998")),
        limit: 7,
    };
    let rows = sqlite_linked_json::scan(&connection, &request).expect("late page");
    assert_eq!(
        rows.iter()
            .map(|r| (r.source.as_str(), r.target.as_str()))
            .collect::<Vec<_>>(),
        [("a", "n09999"), ("z", "")]
    );
    let (sql, parameters) = sqlite_linked_json::query(&request).expect("query");
    let mut plan = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("plan");
    let plan = plan
        .query_map(rusqlite::params_from_iter(parameters.iter()), |r| {
            r.get::<_, String>(3)
        })
        .expect("plan rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("plan text")
        .join("; ");
    let mut statement = connection.prepare(&sql).expect("statement");
    let count = statement
        .query_map(rusqlite::params_from_iter(parameters), |_| Ok(()))
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("values")
        .len();
    assert_eq!(count, 2);
    let steps = statement.get_status(rusqlite::StatementStatus::VmStep);
    assert!(
        steps < 300,
        "prior links were revisited: {steps} VM steps; {plan}"
    );
    println!("10,000 links, late joined page: {steps} VM steps; {plan}");
}

#[test]
fn linked_header_pages_exclude_unrelated_payloads_and_preserve_missing_records() {
    let connection = Connection::open_in_memory().expect("fixture operation succeeds");
    create_tables(&connection).expect("fixture operation succeeds");
    let ops = Ops {
        connection: &connection,
    };
    for key in ["a", "b", "c"] {
        ops.insert(Table::Anchors, Key::Str(key), &[])
            .expect("fixture operation succeeds");
    }
    for (source, target, kind) in [
        ("a", "1", "selected"),
        ("a", "2", "selected"),
        ("b", "3", "selected"),
        ("c", "4", "selected"),
        ("a", "huge", "unrelated"),
    ] {
        ops.insert(
            Table::Relations,
            Key::Str3(source, target, kind),
            &vec![b'x'; 1024 * 1024],
        )
        .expect("fixture operation succeeds");
    }
    for id in ["a", "b", "1", "2", "3"] {
        ops.insert(Table::Nodes, Key::Str(id), &serde_json::to_vec(&serde_json::json!({
            "id": id, "properties": {"label": "Nébula / x:y"}, "discarded": "λ".repeat(1024 * 1024)
        })).expect("fixture operation succeeds")).expect("fixture operation succeeds");
    }
    let mut request = LinkedJsonScan {
        roots: Table::Anchors,
        links: Table::Relations,
        records: Table::Nodes,
        relation: "selected",
        fields: &["id", "properties.label", "absent"],
        after: None,
        limit: 2,
    };
    let first =
        sqlite_linked_json::scan(&connection, &request).expect("fixture operation succeeds");
    assert_eq!(
        first
            .iter()
            .map(|row| (row.source.as_str(), row.target.as_str()))
            .collect::<Vec<_>>(),
        [("a", "1"), ("a", "2")]
    );
    let bytes: usize = first
        .iter()
        .flat_map(|r| [&r.source_json, &r.target_json])
        .flatten()
        .map(Vec::len)
        .sum();
    assert!(
        bytes < 512,
        "only projected headers cross the seam: {bytes}"
    );
    let header: serde_json::Value = serde_json::from_slice(
        first[0]
            .target_json
            .as_ref()
            .expect("fixture operation succeeds"),
    )
    .expect("fixture operation succeeds");
    assert_eq!(
        header,
        serde_json::json!({"id":"1","properties.label":"Nébula / x:y","absent":null})
    );
    request.after = Some(("a", "2"));
    let second =
        sqlite_linked_json::scan(&connection, &request).expect("fixture operation succeeds");
    assert_eq!(
        second
            .iter()
            .map(|row| (row.source.as_str(), row.target.as_str()))
            .collect::<Vec<_>>(),
        [("b", "3"), ("c", "4")]
    );
    assert!(second[1].source_json.is_none() && second[1].target_json.is_none());
    request.after = Some(("c", "4"));
    assert!(
        sqlite_linked_json::scan(&connection, &request)
            .expect("fixture operation succeeds")
            .is_empty()
    );
    request.after = None;
    request.limit = 0;
    assert!(
        sqlite_linked_json::scan(&connection, &request)
            .expect("fixture operation succeeds")
            .is_empty()
    );
}

#[test]
fn linked_projection_rejects_wrong_table_shapes_and_corrupt_selected_json() {
    let connection = Connection::open_in_memory().expect("fixture operation succeeds");
    create_tables(&connection).expect("fixture operation succeeds");
    let mut request = LinkedJsonScan {
        roots: Table::Anchors,
        links: Table::Relations,
        records: Table::Nodes,
        relation: "selected",
        fields: &["id"],
        after: None,
        limit: 8,
    };
    for (roots, links, records) in [
        (Table::Relations, Table::Relations, Table::Nodes),
        (Table::Anchors, Table::Nodes, Table::Nodes),
        (Table::Anchors, Table::Relations, Table::Cards),
    ] {
        assert!(
            sqlite_linked_json::scan(
                &connection,
                &LinkedJsonScan {
                    roots,
                    links,
                    records,
                    ..request
                }
            )
            .is_err()
        );
    }
    let ops = Ops {
        connection: &connection,
    };
    ops.insert(Table::Anchors, Key::Str("a"), &[])
        .expect("fixture operation succeeds");
    ops.insert(Table::Nodes, Key::Str("a"), b"not-json")
        .expect("fixture operation succeeds");
    ops.insert(
        Table::Relations,
        Key::Str3("a", "b", "selected"),
        b"not decoded",
    )
    .expect("fixture operation succeeds");
    assert!(sqlite_linked_json::scan(&connection, &request).is_err());
    request.relation = "selected' OR 1=1 --";
    assert!(
        sqlite_linked_json::scan(&connection, &request)
            .expect("fixture operation succeeds")
            .is_empty()
    );
}
