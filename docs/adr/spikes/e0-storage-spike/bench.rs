//! E0 storage spike: redb vs fjall vs SQLite (rusqlite, bundled) on a
//! KMP-embedded-shaped workload.
//!
//! Workload per event: append event body (~1KB JSON) + dedup index entry
//! (event_id -> seq) + upsert one node detail (~400B) + insert two adjacency
//! edges (~150B each). This mirrors the embedded edition's write path:
//! ContextEventStore + ProcessedEventStore + synchronous projection
//! (NodeDetailReader / NodeRelationshipReader backing stores).
//!
//! Phases:
//!   1. per-event durable ingest (fsync per event commit)   - interactive writes
//!   2. batched ingest (1000 events/commit)                 - import / replay
//!   3. reopen (close, open, one point read)                - session start
//!   4. random point reads (node details)                   - kernel_inspect
//!   5. adjacency prefix scans                              - load_neighborhood
//!   6. on-disk size

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const PER_EVENT_DURABLE: u64 = 2_000;
const BATCHED_TOTAL: u64 = 100_000;
const BATCH: u64 = 1_000;
const NODES: u64 = 20_000;
const POINT_READS: u64 = 10_000;
const ADJ_SCANS: u64 = 1_000;

const TOTAL_EVENTS: u64 = PER_EVENT_DURABLE + BATCHED_TOTAL;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

fn node_name(node: u64) -> String {
    format!("node-{node:08}")
}

fn node_of(seq: u64) -> u64 {
    seq % NODES
}

fn event_id(seq: u64) -> String {
    format!("ev-{seq:012}")
}

fn event_json(seq: u64) -> String {
    let filler = "kernel memory context event body padding ".repeat(18);
    format!(
        r#"{{"event_id":"{}","about":"about-{:03}","occurred_at":"2026-07-21T{:02}:{:02}:{:02}Z","dimensions":["temporal","project","decision"],"node_id":"{}","payload":"{}","seq":{}}}"#,
        event_id(seq),
        seq % 200,
        seq % 24,
        seq % 60,
        (seq / 60) % 60,
        node_name(node_of(seq)),
        filler,
        seq
    )
}

fn detail_json(node: u64, seq: u64) -> String {
    let filler = "node detail summary text ".repeat(12);
    format!(
        r#"{{"node_id":"{}","last_seq":{},"summary":"{}","tier":"L1"}}"#,
        node_name(node),
        seq,
        filler
    )
}

fn edge_json(src: u64, dst: u64, seq: u64) -> String {
    format!(
        r#"{{"src":"{}","dst":"{}","type":"relates_to","seq":{},"confidence":0.9,"rationale":"spike edge"}}"#,
        node_name(src),
        node_name(dst),
        seq
    )
}

fn edge_targets(seq: u64) -> [u64; 2] {
    let mut lcg = Lcg(seq.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    [lcg.next() % NODES, lcg.next() % NODES]
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size_bytes(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

#[derive(Debug)]
struct Results {
    name: &'static str,
    per_event_evps: f64,
    batched_evps: f64,
    reopen_ms: f64,
    point_reads_ps: f64,
    adj_scans_ps: f64,
    adj_edges_seen: u64,
    size_mb: f64,
}

fn fresh_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------- redb

mod redb_bench {
    use super::*;
    use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

    const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("events");
    const DEDUP: TableDefinition<&str, u64> = TableDefinition::new("dedup");
    const DETAILS: TableDefinition<&str, &[u8]> = TableDefinition::new("details");
    const EDGES: TableDefinition<(&str, u64), &[u8]> = TableDefinition::new("edges");

    fn apply_event(tx: &redb::WriteTransaction, seq: u64) {
        let mut events = tx.open_table(EVENTS).unwrap();
        let mut dedup = tx.open_table(DEDUP).unwrap();
        let mut details = tx.open_table(DETAILS).unwrap();
        let mut edges = tx.open_table(EDGES).unwrap();

        events.insert(seq, event_json(seq).as_bytes()).unwrap();
        dedup.insert(event_id(seq).as_str(), seq).unwrap();
        let node = node_of(seq);
        details
            .insert(
                node_name(node).as_str(),
                detail_json(node, seq).as_bytes(),
            )
            .unwrap();
        for (i, dst) in edge_targets(seq).into_iter().enumerate() {
            edges
                .insert(
                    (node_name(node).as_str(), seq * 2 + i as u64),
                    edge_json(node, dst, seq).as_bytes(),
                )
                .unwrap();
        }
    }

    pub fn run(root: &Path) -> Results {
        let dir = fresh_dir(root, "redb");
        let file = dir.join("kernel.redb");
        let db = Database::create(&file).unwrap();

        let t = Instant::now();
        for seq in 0..PER_EVENT_DURABLE {
            let tx = db.begin_write().unwrap();
            apply_event(&tx, seq);
            tx.commit().unwrap();
        }
        let per_event_evps = PER_EVENT_DURABLE as f64 / t.elapsed().as_secs_f64();

        let t = Instant::now();
        let mut seq = PER_EVENT_DURABLE;
        while seq < TOTAL_EVENTS {
            let tx = db.begin_write().unwrap();
            for s in seq..(seq + BATCH).min(TOTAL_EVENTS) {
                apply_event(&tx, s);
            }
            tx.commit().unwrap();
            seq += BATCH;
        }
        let batched_evps = BATCHED_TOTAL as f64 / t.elapsed().as_secs_f64();

        drop(db);

        let t = Instant::now();
        let db = Database::open(&file).unwrap();
        {
            let rx = db.begin_read().unwrap();
            let details = rx.open_table(DETAILS).unwrap();
            let v = details.get(node_name(0).as_str()).unwrap();
            assert!(v.is_some());
        }
        let reopen_ms = t.elapsed().as_secs_f64() * 1000.0;

        let rx = db.begin_read().unwrap();
        let details = rx.open_table(DETAILS).unwrap();
        let mut lcg = Lcg(42);
        let t = Instant::now();
        let mut hits = 0u64;
        for _ in 0..POINT_READS {
            let node = lcg.next() % NODES;
            if details.get(node_name(node).as_str()).unwrap().is_some() {
                hits += 1;
            }
        }
        let point_reads_ps = POINT_READS as f64 / t.elapsed().as_secs_f64();
        assert!(hits > POINT_READS * 9 / 10);

        let edges = rx.open_table(EDGES).unwrap();
        let mut lcg = Lcg(7);
        let mut edge_count = 0u64;
        let t = Instant::now();
        for _ in 0..ADJ_SCANS {
            let node = node_name(lcg.next() % NODES);
            let range = edges
                .range((node.as_str(), 0)..=(node.as_str(), u64::MAX))
                .unwrap();
            for entry in range {
                let _ = entry.unwrap();
                edge_count += 1;
            }
        }
        let adj_scans_ps = ADJ_SCANS as f64 / t.elapsed().as_secs_f64();

        Results {
            name: "redb",
            per_event_evps,
            batched_evps,
            reopen_ms,
            point_reads_ps,
            adj_scans_ps,
            adj_edges_seen: edge_count,
            size_mb: dir_size_bytes(&dir) as f64 / 1e6,
        }
    }
}

// ---------------------------------------------------------------- fjall

mod fjall_bench {
    use super::*;
    use fjall::{Database, KeyspaceCreateOptions, PersistMode};

    fn open_db(dir: &Path) -> Database {
        Database::create_or_recover(fjall::Config::new(dir)).unwrap()
    }

    pub fn run(root: &Path) -> Results {
        let dir = fresh_dir(root, "fjall");
        let db = open_db(&dir);
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();
        let dedup = db.keyspace("dedup", KeyspaceCreateOptions::default).unwrap();
        let details = db
            .keyspace("details", KeyspaceCreateOptions::default)
            .unwrap();
        let edges = db.keyspace("edges", KeyspaceCreateOptions::default).unwrap();

        let apply = |seq: u64| {
            events
                .insert(seq.to_be_bytes(), event_json(seq).as_bytes())
                .unwrap();
            dedup
                .insert(event_id(seq).as_bytes(), &seq.to_be_bytes())
                .unwrap();
            let node = node_of(seq);
            details
                .insert(node_name(node).as_bytes(), detail_json(node, seq).as_bytes())
                .unwrap();
            for (i, dst) in edge_targets(seq).into_iter().enumerate() {
                let key = format!("{}\x1f{:016x}", node_name(node), seq * 2 + i as u64);
                edges
                    .insert(key.as_bytes(), edge_json(node, dst, seq).as_bytes())
                    .unwrap();
            }
        };

        let t = Instant::now();
        for seq in 0..PER_EVENT_DURABLE {
            apply(seq);
            db.persist(PersistMode::SyncAll).unwrap();
        }
        let per_event_evps = PER_EVENT_DURABLE as f64 / t.elapsed().as_secs_f64();

        let t = Instant::now();
        let mut seq = PER_EVENT_DURABLE;
        while seq < TOTAL_EVENTS {
            for s in seq..(seq + BATCH).min(TOTAL_EVENTS) {
                apply(s);
            }
            db.persist(PersistMode::SyncAll).unwrap();
            seq += BATCH;
        }
        let batched_evps = BATCHED_TOTAL as f64 / t.elapsed().as_secs_f64();

        drop(events);
        drop(dedup);
        drop(details);
        drop(edges);
        drop(db);

        let t = Instant::now();
        let db = open_db(&dir);
        let details = db
            .keyspace("details", KeyspaceCreateOptions::default)
            .unwrap();
        let v = details.get(node_name(0).as_bytes()).unwrap();
        assert!(v.is_some());
        let reopen_ms = t.elapsed().as_secs_f64() * 1000.0;

        let edges = db.keyspace("edges", KeyspaceCreateOptions::default).unwrap();

        let mut lcg = Lcg(42);
        let t = Instant::now();
        let mut hits = 0u64;
        for _ in 0..POINT_READS {
            let node = lcg.next() % NODES;
            if details.get(node_name(node).as_bytes()).unwrap().is_some() {
                hits += 1;
            }
        }
        let point_reads_ps = POINT_READS as f64 / t.elapsed().as_secs_f64();
        assert!(hits > POINT_READS * 9 / 10);

        let mut lcg = Lcg(7);
        let mut edge_count = 0u64;
        let t = Instant::now();
        for _ in 0..ADJ_SCANS {
            let prefix = format!("{}\x1f", node_name(lcg.next() % NODES));
            for guard in edges.prefix(prefix.as_bytes()) {
                let _ = guard.value().unwrap();
                edge_count += 1;
            }
        }
        let adj_scans_ps = ADJ_SCANS as f64 / t.elapsed().as_secs_f64();

        Results {
            name: "fjall",
            per_event_evps,
            batched_evps,
            reopen_ms,
            point_reads_ps,
            adj_scans_ps,
            adj_edges_seen: edge_count,
            size_mb: dir_size_bytes(&dir) as f64 / 1e6,
        }
    }
}

// ---------------------------------------------------------------- sqlite

mod sqlite_bench {
    use super::*;
    use rusqlite::Connection;

    fn open(dir: &Path) -> Connection {
        let conn = Connection::open(dir.join("kernel.sqlite3")).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "synchronous", "FULL").unwrap();
        conn
    }

    fn apply_event(conn: &Connection, seq: u64) {
        let node = node_of(seq);
        conn.prepare_cached("INSERT INTO events(seq, body) VALUES (?1, ?2)")
            .unwrap()
            .execute(rusqlite::params![seq as i64, event_json(seq).as_bytes()])
            .unwrap();
        conn.prepare_cached("INSERT INTO dedup(event_id, seq) VALUES (?1, ?2)")
            .unwrap()
            .execute(rusqlite::params![event_id(seq), seq as i64])
            .unwrap();
        conn.prepare_cached(
            "INSERT INTO details(node_id, body) VALUES (?1, ?2) \
             ON CONFLICT(node_id) DO UPDATE SET body = excluded.body",
        )
        .unwrap()
        .execute(rusqlite::params![
            node_name(node),
            detail_json(node, seq).as_bytes()
        ])
        .unwrap();
        for (i, dst) in edge_targets(seq).into_iter().enumerate() {
            conn.prepare_cached("INSERT INTO edges(src, seq, body) VALUES (?1, ?2, ?3)")
                .unwrap()
                .execute(rusqlite::params![
                    node_name(node),
                    (seq * 2 + i as u64) as i64,
                    edge_json(node, dst, seq).as_bytes()
                ])
                .unwrap();
        }
    }

    pub fn run(root: &Path) -> Results {
        let dir = fresh_dir(root, "sqlite");
        let conn = open(&dir);
        conn.execute_batch(
            "CREATE TABLE events(seq INTEGER PRIMARY KEY, body BLOB NOT NULL);
             CREATE TABLE dedup(event_id TEXT PRIMARY KEY, seq INTEGER NOT NULL);
             CREATE TABLE details(node_id TEXT PRIMARY KEY, body BLOB NOT NULL);
             CREATE TABLE edges(src TEXT NOT NULL, seq INTEGER NOT NULL, body BLOB NOT NULL,
                                PRIMARY KEY(src, seq)) WITHOUT ROWID;",
        )
        .unwrap();

        let t = Instant::now();
        for seq in 0..PER_EVENT_DURABLE {
            conn.execute_batch("BEGIN IMMEDIATE").unwrap();
            apply_event(&conn, seq);
            conn.execute_batch("COMMIT").unwrap();
        }
        let per_event_evps = PER_EVENT_DURABLE as f64 / t.elapsed().as_secs_f64();

        let t = Instant::now();
        let mut seq = PER_EVENT_DURABLE;
        while seq < TOTAL_EVENTS {
            conn.execute_batch("BEGIN IMMEDIATE").unwrap();
            for s in seq..(seq + BATCH).min(TOTAL_EVENTS) {
                apply_event(&conn, s);
            }
            conn.execute_batch("COMMIT").unwrap();
            seq += BATCH;
        }
        let batched_evps = BATCHED_TOTAL as f64 / t.elapsed().as_secs_f64();

        drop(conn);

        let t = Instant::now();
        let conn = open(&dir);
        let body: Vec<u8> = conn
            .query_row(
                "SELECT body FROM details WHERE node_id = ?1",
                [node_name(0)],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!body.is_empty());
        let reopen_ms = t.elapsed().as_secs_f64() * 1000.0;

        let mut lcg = Lcg(42);
        let t = Instant::now();
        let mut hits = 0u64;
        for _ in 0..POINT_READS {
            let node = lcg.next() % NODES;
            let found: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT body FROM details WHERE node_id = ?1",
                    [node_name(node)],
                    |row| row.get(0),
                )
                .ok();
            if found.is_some() {
                hits += 1;
            }
        }
        let point_reads_ps = POINT_READS as f64 / t.elapsed().as_secs_f64();
        assert!(hits > POINT_READS * 9 / 10);

        let mut lcg = Lcg(7);
        let mut edge_count = 0u64;
        let t = Instant::now();
        for _ in 0..ADJ_SCANS {
            let node = node_name(lcg.next() % NODES);
            let mut stmt = conn
                .prepare_cached("SELECT body FROM edges WHERE src = ?1")
                .unwrap();
            let rows = stmt
                .query_map([node], |row| row.get::<_, Vec<u8>>(0))
                .unwrap();
            for row in rows {
                let _ = row.unwrap();
                edge_count += 1;
            }
        }
        let adj_scans_ps = ADJ_SCANS as f64 / t.elapsed().as_secs_f64();

        Results {
            name: "sqlite",
            per_event_evps,
            batched_evps,
            reopen_ms,
            point_reads_ps,
            adj_scans_ps,
            adj_edges_seen: edge_count,
            size_mb: dir_size_bytes(&dir) as f64 / 1e6,
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("usage: bench <data-dir>"));
    fs::create_dir_all(&root).unwrap();

    eprintln!(
        "corpus: {TOTAL_EVENTS} events ({PER_EVENT_DURABLE} per-event durable + {BATCHED_TOTAL} batched x{BATCH}), {NODES} nodes, ~1KB event bodies"
    );

    let all = [
        redb_bench::run(&root),
        fjall_bench::run(&root),
        sqlite_bench::run(&root),
    ];

    println!("| engine | per-event durable (ev/s) | batched (ev/s) | reopen (ms) | point reads (r/s) | adjacency scans (scan/s) | edges seen | size (MB) |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for r in &all {
        println!(
            "| {} | {:.0} | {:.0} | {:.1} | {:.0} | {:.0} | {} | {:.1} |",
            r.name,
            r.per_event_evps,
            r.batched_evps,
            r.reopen_ms,
            r.point_reads_ps,
            r.adj_scans_ps,
            r.adj_edges_seen,
            r.size_mb
        );
    }
}
