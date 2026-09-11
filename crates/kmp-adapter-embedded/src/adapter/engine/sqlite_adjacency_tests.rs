use super::*;

#[test]
fn typed_pages_seek_past_unrelated_fanout_in_both_directions() {
    let connection = Connection::open_in_memory().expect("database");
    create_tables(&connection).expect("schema with typed indexes");
    for table in [Table::Relations, Table::RelationsByTarget] {
        let mut insert = connection
            .prepare(&format!(
                "INSERT INTO \"{}\" VALUES ('seed', ?, ?, ?)",
                table
            ))
            .expect("insert");
        for i in 0..10_000 {
            insert
                .execute(params![
                    format!("node-{i:05}"),
                    "depends_on",
                    vec![42u8; 16]
                ])
                .expect("noise");
            if i % 1000 == 0 {
                insert
                    .execute(params![
                        format!("node-{i:05}"),
                        "contains_entry",
                        vec![7u8; 16]
                    ])
                    .expect("coordinate");
            }
        }
        let ops = Ops {
            connection: &connection,
        };
        let rows = ops
            .scan_str3_page(
                table,
                "seed",
                Some(("node-06000", "contains_entry")),
                2,
                Some("contains_entry"),
            )
            .expect("typed page");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0.1, "node-07000");
        assert_eq!(rows[1].0.1, "node-08000");
        let sql = adjacency_page_sql(table, true, true);
        let plan: String = connection
            .query_row(
                &format!("EXPLAIN QUERY PLAN {sql}"),
                params!["seed", "contains_entry", "node-06000", 2],
                |r| r.get(3),
            )
            .expect("plan");
        assert!(
            plan.contains("SEARCH") && plan.contains("by_kind"),
            "{plan}"
        );
        let mut statement = connection.prepare(&sql).expect("statement");
        let values = statement
            .query_map(params!["seed", "contains_entry", "node-06000", 2], |r| {
                r.get::<_, Vec<u8>>(3)
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("values");
        assert_eq!(values, vec![vec![7u8; 16]; 2]);
        let steps = statement.get_status(rusqlite::StatementStatus::VmStep);
        assert!(steps < 100, "{steps} VM steps; {plan}");
        println!(
            "{}: 10,000 unrelated rows, typed late page of 2: {steps} VM steps; {plan}",
            table
        );
    }
}

#[test]
fn a_late_page_seeks_the_key_and_reads_only_its_bounded_values() {
    let connection = Connection::open_in_memory().expect("database");
    connection.execute_batch(
        "CREATE TABLE relations_by_source(k1 TEXT, k2 TEXT, k3 TEXT, v BLOB, PRIMARY KEY(k1,k2,k3)) WITHOUT ROWID"
    ).expect("table");
    {
        let mut insert = connection
            .prepare("INSERT INTO relations_by_source VALUES ('seed', ?, 'depends_on', ?)")
            .expect("insert");
        for i in 0..10_000 {
            insert
                .execute(params![format!("node-{i:05}"), vec![42u8; 16]])
                .expect("row");
        }
    }
    let ops = Ops {
        connection: &connection,
    };
    let rows = ops
        .scan_str3_page(
            Table::Relations,
            "seed",
            Some(("node-09000", "depends_on")),
            7,
            None,
        )
        .expect("page");
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[0].0.1, "node-09001");
    assert_eq!(rows[6].0.1, "node-09007");
    assert!(
        ops.scan_str3_page(Table::Relations, "seed", None, 0, None)
            .expect("zero")
            .is_empty()
    );

    // Execute the exact SQL used by Ops. A late page must not walk the first
    // 9,000 neighbors again: inspect both the query plan and VM work.
    let sql = adjacency_page_sql(Table::Relations, true, false);
    let plan: String = connection
        .query_row(
            &format!("EXPLAIN QUERY PLAN {sql}"),
            params!["seed", "node-09000", "depends_on", 7],
            |r| r.get(3),
        )
        .expect("plan");
    assert!(
        plan.contains("SEARCH") && plan.contains("PRIMARY KEY"),
        "{plan}"
    );
    let mut statement = connection.prepare(&sql).expect("statement");
    let records = statement
        .query_map(params!["seed", "node-09000", "depends_on", 7], |row| {
            row.get::<_, Vec<u8>>(3)
        })
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("values");
    assert_eq!(records.len(), 7);
    let steps = statement.get_status(rusqlite::StatementStatus::VmStep);
    assert!(
        steps < 250,
        "late page scanned too much: {steps} VM steps; {plan}"
    );
    println!("10,000 neighbors, late page of7: {steps} VM steps; {plan}");
}
