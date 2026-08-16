//! E-concurrency spike — can two agent hosts share one embedded store?
//!
//! ADR-011 assumed concurrent sessions were "realistic but occasional" and
//! leaned on redb being single-process by design. Two *different* agent
//! hosts (Codex and Claude Code, each running the KMP plugin) is now the
//! ordinary desktop, so the question is no longer throughput under
//! contention — it is whether the second host works at all.
//!
//! Measures what ADR-009 did not: real OS processes, same store, at once.
//!
//! Usage:
//!   spike <dir>                  run the whole comparison (spawns children)
//!   spike <dir> child <engine> <id> <events>   one writer process
//!   spike <dir> reader <engine> <seconds>      one reader process

use redb::ReadableDatabase;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EVENT_BODY: usize = 1024;

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "./data".into()));
    std::fs::create_dir_all(&dir).expect("create dir");

    match args.get(2).map(String::as_str) {
        Some("child") => {
            let engine = args[3].as_str();
            let id: u32 = args[4].parse().unwrap();
            let events: u32 = args[5].parse().unwrap();
            child_writer(&dir, engine, id, events);
        }
        Some("reader") => {
            let engine = args[3].as_str();
            let seconds: u64 = args[4].parse().unwrap();
            child_reader(&dir, engine, seconds);
        }
        Some("bench") => bench(&dir),
        _ => orchestrate(&dir),
    }
}

// ---------------------------------------------------------------- engines --

fn redb_path(dir: &Path) -> PathBuf {
    dir.join("kernel.redb")
}
fn sqlite_path(dir: &Path) -> PathBuf {
    dir.join("kernel.sqlite3")
}

fn sqlite_open(path: &Path) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(path).expect("open sqlite");
    // busy_timeout FIRST. Switching journal mode takes a brief exclusive
    // lock, so two processes opening at the same moment collide there —
    // before WAL is even in effect. Without the timeout already armed, the
    // loser gets SQLITE_BUSY instead of waiting a few milliseconds.
    connection
        .busy_timeout(Duration::from_secs(10))
        .expect("busy timeout");
    // WAL is the whole point: readers do not block the writer and writers
    // from other processes queue instead of being refused.
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .expect("wal");
    connection
        .pragma_update(None, "synchronous", "FULL")
        .expect("synchronous");
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS event_log (
                 seq INTEGER PRIMARY KEY AUTOINCREMENT,
                 writer INTEGER NOT NULL,
                 body BLOB NOT NULL
             );",
        )
        .expect("schema");
    connection
}

// ------------------------------------------------------------------ child --

fn child_writer(dir: &Path, engine: &str, id: u32, events: u32) {
    let body = vec![b'x'; EVENT_BODY];
    let started = Instant::now();
    let mut written = 0u32;

    match engine {
        "redb" => {
            let database = match redb::Database::create(redb_path(dir)) {
                Ok(database) => database,
                Err(error) => {
                    println!("RESULT writer={id} engine=redb opened=false written=0 error={error}");
                    return;
                }
            };
            let table: redb::TableDefinition<u64, &[u8]> = redb::TableDefinition::new("event_log");
            for index in 0..events {
                let transaction = database.begin_write().expect("begin");
                {
                    let mut open = transaction.open_table(table).expect("table");
                    let key = u64::from(id) * 1_000_000 + u64::from(index);
                    open.insert(key, body.as_slice()).expect("insert");
                }
                transaction.commit().expect("commit");
                written += 1;
            }
        }
        _ => {
            let connection = sqlite_open(&sqlite_path(dir));
            for _ in 0..events {
                match connection.execute(
                    "INSERT INTO event_log (writer, body) VALUES (?1, ?2)",
                    rusqlite::params![id, body.as_slice()],
                ) {
                    Ok(_) => written += 1,
                    Err(error) => {
                        println!(
                            "RESULT writer={id} engine=sqlite opened=true written={written} error={error}"
                        );
                        return;
                    }
                }
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "RESULT writer={id} engine={engine} opened=true written={written} seconds={elapsed:.3}"
    );
}

fn child_reader(dir: &Path, engine: &str, seconds: u64) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut reads = 0u64;
    let mut last_seen = 0i64;

    if engine == "sqlite" {
        let connection = sqlite_open(&sqlite_path(dir));
        while Instant::now() < deadline {
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM event_log", [], |row| row.get(0))
                .expect("count");
            if count < last_seen {
                println!("RESULT reader=1 engine=sqlite monotonic=false");
                return;
            }
            last_seen = count;
            reads += 1;
        }
        println!("RESULT reader=1 engine=sqlite reads={reads} last={last_seen} monotonic=true");
    }
}

// ------------------------------------------------------------- orchestrate --

fn spawn(dir: &Path, args: &[&str]) -> std::process::Child {
    let executable = env::current_exe().expect("current exe");
    Command::new(executable)
        .arg(dir)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn child")
}

fn collect(children: Vec<std::process::Child>) -> Vec<String> {
    let mut lines = Vec::new();
    for child in children {
        let output = child.wait_with_output().expect("wait");
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.starts_with("RESULT") {
                lines.push(line.to_string());
            }
        }
    }
    lines
}

fn orchestrate(dir: &Path) {
    const WRITERS: u32 = 2;
    const EVENTS: u32 = 300;

    println!("# two OS processes, one store, {EVENTS} durable events each\n");

    for engine in ["redb", "sqlite"] {
        let engine_dir = dir.join(engine);
        std::fs::create_dir_all(&engine_dir).expect("engine dir");

        let started = Instant::now();
        let events = EVENTS.to_string();
        let children: Vec<_> = (1..=WRITERS)
            .map(|id| {
                let id = id.to_string();
                spawn(&engine_dir, &["child", engine, &id, &events])
            })
            .collect();
        let results = collect(children);
        let wall = started.elapsed().as_secs_f64();

        let opened = results.iter().filter(|line| line.contains("opened=true")).count();
        let total: u32 = results
            .iter()
            .filter_map(|line| {
                line.split_whitespace()
                    .find(|field| field.starts_with("written="))
                    .and_then(|field| field.trim_start_matches("written=").parse::<u32>().ok())
            })
            .sum();

        println!("## {engine}");
        for line in &results {
            println!("  {line}");
        }
        println!(
            "  processes that opened the store: {opened}/{WRITERS}\n  \
             events durably written: {total}/{}\n  wall clock: {wall:.3}s\n",
            WRITERS * EVENTS
        );
    }

    // A reader alongside a live writer, sqlite only: redb never got here.
    let engine_dir = dir.join("sqlite-reader");
    std::fs::create_dir_all(&engine_dir).expect("engine dir");
    let writer = spawn(&engine_dir, &["child", "sqlite", "1", "400"]);
    let reader = spawn(&engine_dir, &["reader", "sqlite", "2"]);
    println!("## sqlite — reader concurrent with writer");
    for line in collect(vec![writer, reader]) {
        println!("  {line}");
    }
}

// ------------------------------------------------------------------ bench --
//
// Re-measures ADR-009's deciding criteria on the machine deciding today.
// The original numbers are from different hardware and a different kernel,
// so the trade-off being accepted has to be priced here, not quoted.

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn bench(dir: &Path) {
    const EVENTS: u32 = 20_000;
    const BATCH: u32 = 1_000;
    const READS: u32 = 200_000;

    let body = vec![b'x'; EVENT_BODY];

    // ---- redb -----------------------------------------------------------
    let redb_dir = dir.join("bench-redb");
    std::fs::create_dir_all(&redb_dir).expect("dir");
    let table: redb::TableDefinition<u64, &[u8]> = redb::TableDefinition::new("event_log");

    let started = Instant::now();
    {
        let database = redb::Database::create(redb_path(&redb_dir)).expect("create");
        for chunk in 0..(EVENTS / BATCH) {
            let transaction = database.begin_write().expect("begin");
            {
                let mut open = transaction.open_table(table).expect("table");
                for index in 0..BATCH {
                    open.insert(u64::from(chunk * BATCH + index), body.as_slice())
                        .expect("insert");
                }
            }
            transaction.commit().expect("commit");
        }
    }
    let redb_write = started.elapsed().as_secs_f64();

    let started = Instant::now();
    let database = redb::Database::open(redb_path(&redb_dir)).expect("open");
    let redb_reopen = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    {
        let transaction = database.begin_read().expect("read");
        let open = transaction.open_table(table).expect("table");
        for index in 0..READS {
            let key = u64::from(index % EVENTS);
            let value = open.get(key).expect("get");
            assert!(value.is_some());
        }
    }
    let redb_reads = f64::from(READS) / started.elapsed().as_secs_f64();
    drop(database);
    let redb_size = dir_size(&redb_dir) as f64 / 1_048_576.0;

    // ---- sqlite ---------------------------------------------------------
    let sqlite_dir = dir.join("bench-sqlite");
    std::fs::create_dir_all(&sqlite_dir).expect("dir");

    let started = Instant::now();
    {
        let connection = sqlite_open(&sqlite_path(&sqlite_dir));
        for chunk in 0..(EVENTS / BATCH) {
            let transaction = connection.unchecked_transaction().expect("begin");
            {
                let mut statement = transaction
                    .prepare("INSERT INTO event_log (seq, writer, body) VALUES (?1, 0, ?2)")
                    .expect("prepare");
                for index in 0..BATCH {
                    statement
                        .execute(rusqlite::params![chunk * BATCH + index, body.as_slice()])
                        .expect("insert");
                }
            }
            transaction.commit().expect("commit");
        }
    }
    let sqlite_write = started.elapsed().as_secs_f64();

    let started = Instant::now();
    let connection = sqlite_open(&sqlite_path(&sqlite_dir));
    let sqlite_reopen = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    {
        let mut statement = connection
            .prepare("SELECT body FROM event_log WHERE seq = ?1")
            .expect("prepare");
        for index in 0..READS {
            let key = index % EVENTS;
            let found: Result<Vec<u8>, _> = statement.query_row([key], |row| row.get(0));
            assert!(found.is_ok());
        }
    }
    let sqlite_reads = f64::from(READS) / started.elapsed().as_secs_f64();
    drop(connection);
    let sqlite_size = dir_size(&sqlite_dir) as f64 / 1_048_576.0;

    println!("corpus: {EVENTS} events, {EVENT_BODY}B bodies, batches of {BATCH}, {READS} point reads\n");
    println!("| engine | batched write (ev/s) | reopen (ms) | point reads (r/s) | size (MB) |");
    println!("| --- | --- | --- | --- | --- |");
    println!(
        "| redb 4.1.0 | {:.0} | {redb_reopen:.2} | {redb_reads:.0} | {redb_size:.1} |",
        f64::from(EVENTS) / redb_write
    );
    println!(
        "| SQLite (WAL, synchronous=FULL) | {:.0} | {sqlite_reopen:.2} | {sqlite_reads:.0} | {sqlite_size:.1} |",
        f64::from(EVENTS) / sqlite_write
    );
}
