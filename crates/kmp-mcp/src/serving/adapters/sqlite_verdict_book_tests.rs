use serde_json::json;

use super::*;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::verdict_state_digest::StateDigest;
use crate::serving::verdict_template::VerdictTemplate;

const RERANK: VerdictTemplate = VerdictTemplate {
    id: "rerank",
    version: 1,
};

fn question(n: usize) -> JudgementQuestion {
    JudgementQuestion::Noul {
        instructions: json!({"passage": format!("passage {n}"), "question": "q"}),
    }
}

fn key(template: VerdictTemplate, n: usize) -> VerdictKey {
    VerdictKey::of(
        "jev-1",
        template,
        &question(n),
        &StateDigest::of(&json!("q")),
    )
}

fn verdict(yes: f64) -> Verdict {
    Verdict::of(&question(0), &JudgementAnswer::Noul { yes }, 10, 1).expect("fits")
}

fn open(dir: &tempfile::TempDir) -> SqliteVerdictBook {
    SqliteVerdictBook::open(&dir.path().join("judgements.sqlite3"), 1024 * 1024).expect("book")
}

#[test]
fn a_recorded_verdict_reads_back_and_an_unknown_key_is_none() {
    let dir = tempfile::tempdir().expect("dir");
    let book = open(&dir);
    let held = book
        .record(&[(key(RERANK, 1), verdict(0.25))])
        .expect("record");
    assert_eq!(held, vec![verdict(0.25)]);
    assert_eq!(
        book.read(&[key(RERANK, 1), key(RERANK, 2)]).expect("read"),
        vec![Some(verdict(0.25)), None]
    );
}

#[test]
fn the_first_verdict_wins_across_processes() {
    let dir = tempfile::tempdir().expect("dir");
    let first = open(&dir);
    let second = open(&dir);
    first
        .record(&[(key(RERANK, 1), verdict(0.9))])
        .expect("first");
    let held = second
        .record(&[
            (key(RERANK, 1), verdict(0.1)),
            (key(RERANK, 2), verdict(0.4)),
        ])
        .expect("second");
    assert_eq!(held, vec![verdict(0.9), verdict(0.4)]);
    assert_eq!(first.len().expect("len"), 2);
    assert_eq!(
        first.read(&[key(RERANK, 1)]).expect("read"),
        vec![Some(verdict(0.9))]
    );
}

#[test]
fn concurrent_writers_leave_one_verdict_per_key() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("judgements.sqlite3");
    let handles = (0..4)
        .map(|writer| {
            let path = path.clone();
            std::thread::spawn(move || {
                let book = SqliteVerdictBook::open(&path, 1024 * 1024).expect("book");
                let entries = (0..50)
                    .map(|n| (key(RERANK, n), verdict(writer as f64 / 10.0)))
                    .collect::<Vec<_>>();
                book.record(&entries).expect("record")
            })
        })
        .collect::<Vec<_>>();
    let held = handles
        .into_iter()
        .map(|handle| handle.join().expect("writer"))
        .collect::<Vec<_>>();
    for answers in &held[1..] {
        assert_eq!(answers, &held[0], "every writer reads the first verdict");
    }
    assert_eq!(open(&dir).len().expect("len"), 50);
}

#[test]
fn a_template_version_or_the_whole_template_can_be_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let book = open(&dir);
    let two = VerdictTemplate {
        id: "rerank",
        version: 2,
    };
    let paths = VerdictTemplate {
        id: "paths",
        version: 1,
    };
    book.record(&[
        (key(RERANK, 1), verdict(0.1)),
        (key(two, 1), verdict(0.2)),
        (key(paths, 1), verdict(0.3)),
    ])
    .expect("record");
    assert_eq!(book.invalidate(&RERANK.prefix()).expect("version"), 1);
    assert_eq!(
        book.read(&[key(two, 1)]).expect("read"),
        vec![Some(verdict(0.2))]
    );
    assert_eq!(book.invalidate(&RERANK.id_prefix()).expect("template"), 1);
    assert_eq!(book.len().expect("len"), 1);
    assert_eq!(book.invalidate(&[0xFF; 3]).expect("empty range"), 0);
}

#[test]
fn over_budget_the_least_recently_used_verdicts_go_first() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("judgements.sqlite3");
    let budget = 64 * 1024;
    let book = SqliteVerdictBook::open(&path, budget).expect("book");
    for chunk in 0..20 {
        let entries = (chunk * 100..(chunk + 1) * 100)
            .map(|n| (key(RERANK, n), verdict(0.5)))
            .collect::<Vec<_>>();
        book.record(&entries).expect("record");
    }
    book.collect_garbage().expect("collect");
    assert!(
        book.bytes_used().expect("used") <= budget,
        "{} bytes over a {budget} budget",
        book.bytes_used().expect("used")
    );
    let kept = book.len().expect("len");
    assert!(kept > 0 && kept < 2000, "kept {kept}");
    let file = std::fs::metadata(&path).expect("file").len();
    assert!(file <= 2 * budget + 64 * 1024, "file {file} bytes");
}

#[test]
fn the_file_never_outgrows_its_hard_cap() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("judgements.sqlite3");
    let budget = 32 * 1024;
    let book = SqliteVerdictBook::open(&path, budget).expect("book");
    for chunk in 0..10 {
        // One transaction bigger than the cap: the book makes room or refuses,
        // it never grows the file past twice its budget.
        let entries = (chunk * 1000..(chunk + 1) * 1000)
            .map(|n| (key(RERANK, n), verdict(0.5)))
            .collect::<Vec<_>>();
        let _ = book.record(&entries);
        let file = std::fs::metadata(&path).expect("file").len();
        assert!(file <= 2 * budget + 8 * 4096, "file {file} bytes");
    }
}

#[test]
fn undecodable_bytes_never_win_and_a_newer_book_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("judgements.sqlite3");
    let book = SqliteVerdictBook::open(&path, 1024 * 1024).expect("book");
    let raw = Connection::open(&path).expect("raw");
    raw.execute(
        "INSERT INTO verdicts(key, value, last_used) VALUES (?1, x'00', 0)",
        params![key(RERANK, 7).as_bytes()],
    )
    .expect("junk");
    assert_eq!(book.read(&[key(RERANK, 7)]).expect("read"), vec![None]);
    assert_eq!(
        book.record(&[(key(RERANK, 7), verdict(0.6))])
            .expect("repair"),
        vec![verdict(0.6)]
    );
    raw.execute_batch("PRAGMA user_version = 99;")
        .expect("bump");
    drop(book);
    let refused = SqliteVerdictBook::open(&path, 1024 * 1024)
        .err()
        .expect("refused");
    assert!(refused.contains("newer"), "{refused}");
}

#[test]
fn reads_refresh_a_stale_last_used_day() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("judgements.sqlite3");
    let book = SqliteVerdictBook::open(&path, 1024 * 1024).expect("book");
    book.record(&[(key(RERANK, 1), verdict(0.5))])
        .expect("record");
    let raw = Connection::open(&path).expect("raw");
    raw.execute("UPDATE verdicts SET last_used = 3", [])
        .expect("age");
    book.read(&[key(RERANK, 1)]).expect("read");
    let day: i64 = raw
        .query_row("SELECT last_used FROM verdicts", [], |row| row.get(0))
        .expect("day");
    assert_eq!(day, today());
}

#[test]
fn successor_is_the_end_of_a_prefix_range() {
    assert_eq!(successor(&[1, 2]), Some(vec![1, 3]));
    assert_eq!(successor(&[1, 0xFF]), Some(vec![2]));
    assert_eq!(successor(&[0xFF, 0xFF]), None);
}
