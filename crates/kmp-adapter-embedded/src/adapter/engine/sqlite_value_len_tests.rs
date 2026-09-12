//! The stored-value length probe: what it measures, what it refuses, and
//! which snapshot answers it.

use super::*;

fn seeded() -> Connection {
    let connection = Connection::open_in_memory().expect("database");
    create_tables(&connection).expect("schema");
    connection
}

#[test]
fn an_absent_key_is_none_and_a_stored_empty_value_is_zero() {
    let connection = seeded();
    let ops = Ops {
        connection: &connection,
    };
    assert_eq!(
        ops.value_len(Table::Details, Key::Str("never-written"))
            .expect("an absent key is a normal answer"),
        None
    );
    ops.insert(Table::Details, Key::Str("empty"), &[])
        .expect("empty value");
    assert_eq!(
        ops.value_len(Table::Details, Key::Str("empty"))
            .expect("probe"),
        Some(0),
        "a present empty value is measured as zero, never reported absent"
    );
}

#[test]
fn multibyte_text_is_counted_in_stored_bytes_not_in_characters() {
    let connection = seeded();
    let ops = Ops {
        connection: &connection,
    };
    let stored = "Canónica π — ✓ 記録".as_bytes();
    assert!(
        stored.len() > "Canónica π — ✓ 記録".chars().count(),
        "the fixture must actually be multibyte or it proves nothing"
    );
    ops.insert(Table::Details, Key::Str("utf8"), stored)
        .expect("insert");
    assert_eq!(
        ops.value_len(Table::Details, Key::Str("utf8"))
            .expect("probe"),
        Some(stored.len() as u64)
    );
}

#[test]
fn a_value_spilling_onto_overflow_pages_reports_its_exact_length() {
    let connection = seeded();
    let ops = Ops {
        connection: &connection,
    };
    let stored = vec![7u8; 4 * 1024 * 1024];
    ops.insert(Table::Details, Key::Str("large"), &stored)
        .expect("insert");
    assert_eq!(
        ops.value_len(Table::Details, Key::Str("large"))
            .expect("probe"),
        Some(stored.len() as u64)
    );
}

/// A text value would make `length` count characters. Under-reporting the
/// bytes of a record is worse than refusing to answer for it.
#[test]
fn a_value_stored_as_text_is_refused_instead_of_being_miscounted() {
    let connection = seeded();
    connection
        .execute(
            "INSERT INTO details (k, v) VALUES ('text', ?1)",
            params!["Canónica π"],
        )
        .expect("a text value reaches the column despite its blob affinity");
    let ops = Ops {
        connection: &connection,
    };
    let error = ops
        .value_len(Table::Details, Key::Str("text"))
        .expect_err("a text value must not be measured");
    let message = error.to_string();
    assert!(
        message.contains("details") && message.contains("text"),
        "the refusal must name the table and what it found, got: {message}"
    );
}

#[test]
fn a_key_of_the_wrong_shape_is_reported_as_a_programming_error() {
    let connection = seeded();
    let ops = Ops {
        connection: &connection,
    };
    let error = ops
        .value_len(Table::Details, Key::U64(1))
        .expect_err("details is keyed by a string");
    assert!(error.to_string().contains("keyed by Str"), "got: {error}");
}

/// The seam promises a byte length without loading the value. SQLite keeps
/// that promise through two column flags: `OPFLAG_LENGTHARG` and
/// `OPFLAG_TYPEOFARG` make the column access stop at the record header instead
/// of walking the overflow pages a large value occupies. If a future SQLite
/// stops emitting them, this probe silently starts reading whole bodies, which
/// is the one thing it exists to avoid.
#[test]
fn the_probe_statement_is_answered_from_the_record_header() {
    const LENGTH_ARG: i64 = 0x40;
    const TYPEOF_ARG: i64 = 0x80;

    let connection = seeded();
    let sql = value_len_sql(Table::Details);
    let mut statement = connection
        .prepare(&format!("EXPLAIN {sql}"))
        .expect("the probe statement explains");
    let flags: Vec<i64> = statement
        .query_map(params!["any-key"], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(6)?))
        })
        .expect("opcodes")
        .filter_map(|row| {
            let (opcode, p5) = row.expect("opcode row");
            (opcode == "Column").then_some(p5)
        })
        .collect();

    assert!(
        flags.contains(&LENGTH_ARG),
        "length(v) must read only the record header; column flags were {flags:?}"
    );
    assert!(
        flags.contains(&TYPEOF_ARG),
        "typeof(v) must read only the record header; column flags were {flags:?}"
    );
}

/// The probe has to answer from the transaction that will later read the
/// value, or an admission decision would be taken against a different store
/// than the one that serves it.
#[test]
fn a_pinned_read_probes_the_same_snapshot_it_reads() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = SqliteEngine::open_file(&dir.path().join("kernel.sqlite3")).expect("engine opens");

    let mut write = engine.begin_write().expect("write transaction");
    write
        .insert(Table::Details, Key::Str("body"), b"first")
        .expect("insert");
    Box::new(write).commit().expect("commit");

    let read = engine.begin_read().expect("read transaction");
    assert_eq!(
        read.value_len(Table::Details, Key::Str("body"))
            .expect("probe pins the snapshot"),
        Some(5)
    );

    let mut later = engine.begin_write().expect("second write transaction");
    later
        .insert(
            Table::Details,
            Key::Str("body"),
            b"a decidedly longer replacement",
        )
        .expect("insert");
    Box::new(later).commit().expect("commit");

    assert_eq!(
        read.value_len(Table::Details, Key::Str("body"))
            .expect("probe"),
        Some(5),
        "the pinned read must not see the later commit"
    );
    assert_eq!(
        read.get(Table::Details, Key::Str("body")).expect("read"),
        Some(b"first".to_vec()),
        "probe and read must agree inside one transaction"
    );
    drop(read);

    let after = engine.begin_read().expect("later read transaction");
    assert_eq!(
        after
            .value_len(Table::Details, Key::Str("body"))
            .expect("probe"),
        Some(30),
        "a transaction opened afterwards sees the new length"
    );
}
