use super::*;
use kmp_proto::v1beta1::MemoryEvidence;
use kmp_proto_mapping::v1beta1::{LexicalProfile, LexicalRow};

use crate::serving::ports::lexical_candidates::LexicalCandidates;

fn row(text: &str) -> LexicalRow {
    let item = MemoryEvidence {
        id: format!("entry:{text}"),
        text: text.to_string(),
        source: "log".to_string(),
        ..Default::default()
    };
    LexicalRow::read(&item, &LexicalProfile::new(None))
}

fn meta(position: u64) -> SidecarMeta {
    SidecarMeta {
        version: "v".into(),
        profile: "plain".into(),
        position,
        tail: format!("t{position}"),
    }
}

fn change(rebuilt: bool, rows: Vec<(String, Option<LexicalRow>)>) -> AboutChange {
    AboutChange {
        about: "about".into(),
        rebuilt,
        rows: rows
            .into_iter()
            .map(|(doc, row)| (doc, row.map(|row| row.encode())))
            .collect(),
        ..AboutChange::default()
    }
}

fn open() -> (tempfile::TempDir, SqliteLexicalSidecar) {
    let directory = tempfile::tempdir().expect("dir");
    let sidecar =
        SqliteLexicalSidecar::open(&directory.path().join("lexical.sqlite3")).expect("sidecar");
    (directory, sidecar)
}

#[test]
fn a_commit_lands_only_where_the_writer_read_the_sidecar() {
    let (_directory, sidecar) = open();
    let empty = sidecar.meta().expect("meta");
    assert_eq!(empty, SidecarMeta::default());
    let first = change(true, vec![("entry:a".into(), Some(row("valve froze")))]);
    assert!(
        sidecar
            .commit(&empty, &meta(3), true, &[first])
            .expect("commit")
    );
    assert_eq!(sidecar.meta().expect("meta"), meta(3));
    // Another process moved the sidecar: a writer that read the old meta
    // writes nothing.
    let stale = change(false, vec![("entry:b".into(), Some(row("pump started")))]);
    assert!(
        !sidecar
            .commit(&empty, &meta(4), false, &[stale])
            .expect("refused")
    );
    assert!(sidecar.row("about", "entry:b").expect("row").is_none());
    assert_eq!(sidecar.meta().expect("meta"), meta(3));
}

#[test]
fn rows_move_the_totals_postings_and_frequencies_and_twice_is_once() {
    let (_directory, sidecar) = open();
    let start = sidecar.meta().expect("meta");
    let rows = vec![
        ("entry:a".to_string(), Some(row("reserve valve froze"))),
        ("entry:b".to_string(), Some(row("valve replaced"))),
    ];
    sidecar
        .commit(&start, &meta(1), true, &[change(true, rows)])
        .expect("built");
    let built = sidecar.stats("about").expect("stats").expect("built");
    assert_eq!(built.documents, 2);
    let frequencies =
        LexicalCandidates::frequencies(&sidecar, "about", &["valv".into(), "valve".into()], true)
            .expect("df");
    let valve = frequencies
        .values()
        .find(|frequency| **frequency != (0, 0))
        .copied()
        .expect("the valve term");
    assert_eq!(valve, (2, 2));
    // One row changes, one goes; applying the same change twice is once.
    let update = vec![
        (
            "entry:a".to_string(),
            Some(row("reserve valve froze twice")),
        ),
        ("entry:b".to_string(), None),
    ];
    for position in [2, 3] {
        let held = sidecar.meta().expect("meta");
        sidecar
            .commit(
                &held,
                &meta(position),
                false,
                &[change(false, update.clone())],
            )
            .expect("updated");
    }
    let stats = sidecar.stats("about").expect("stats").expect("kept");
    assert_eq!(stats.documents, 1);
    assert_eq!(
        stats.rows_digest,
        row("reserve valve froze twice").fingerprint(false)
    );
    assert_eq!(
        stats.next_ordinal, 2,
        "a removed candidate's ordinal is not reused"
    );
    let candidates = sidecar
        .candidates("about", &["froze".into(), "replaced".into()])
        .expect("candidates");
    assert_eq!(candidates.keys().collect::<Vec<_>>(), ["entry:a"]);
    assert!(
        LexicalCandidates::frequencies(&sidecar, "about", &["replaced".into()], false)
            .expect("df")
            .values()
            .all(|frequency| *frequency == (0, 0))
    );
    let totals = sidecar
        .totals("about", true)
        .expect("totals")
        .expect("built");
    assert_eq!(totals.documents, 1);
    assert_eq!(totals.direct_length, stats.direct_length(true));
}

#[test]
fn a_term_held_by_many_candidates_splits_into_blocks_in_order() {
    let (_directory, sidecar) = open();
    let start = sidecar.meta().expect("meta");
    let rows = (0..300)
        .map(|index| {
            (
                format!("entry:{index:03}"),
                Some(row(&format!("valve n{index}"))),
            )
        })
        .collect::<Vec<_>>();
    sidecar
        .commit(&start, &meta(1), true, &[change(true, rows)])
        .expect("built");
    let terms =
        LexicalCandidates::frequencies(&sidecar, "about", &["valv".into(), "valve".into()], true)
            .expect("df")
            .into_iter()
            .find(|(_, frequency)| *frequency != (0, 0))
            .map(|(term, _)| term)
            .expect("valve");
    let postings = sidecar.postings("about", &terms).expect("postings");
    assert_eq!(postings.len(), 300);
    assert!(
        postings
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal)
    );
    // Taking one out of the middle and adding one keeps every block in order.
    let held = sidecar.meta().expect("meta");
    let rows = vec![
        ("entry:150".to_string(), None),
        ("entry:new".to_string(), Some(row("valve new"))),
    ];
    sidecar
        .commit(&held, &meta(2), false, &[change(false, rows)])
        .expect("updated");
    let postings = sidecar.postings("about", &terms).expect("postings");
    assert_eq!(postings.len(), 300);
    assert!(
        postings
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal)
    );
    sidecar.forget("about").expect("forgotten");
    assert!(sidecar.stats("about").expect("stats").is_none());
    assert!(
        sidecar
            .postings("about", &terms)
            .expect("postings")
            .is_empty()
    );
}

#[test]
fn lookups_keep_their_plans_and_lifecycle_reads_use_the_type_index() {
    let (_directory, sidecar) = open();
    sidecar
        .with(|connection| {
            // A cached statement is not re-planned (and re-parsed) per binding.
            assert!(
                connection
                    .db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_QPSG)
                    .map_err(storage)?
            );
            // An about's lifecycle relations, and what supports each end, are
            // found through the index, not by scanning all its relations.
            let plan_of = |sql: &str, bound: &[&str]| -> Result<String, String> {
                let mut statement = connection
                    .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                    .map_err(storage)?;
                let rows = statement
                    .query_map(rusqlite::params_from_iter(bound), |row| {
                        row.get::<_, String>(3)
                    })
                    .map_err(storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage)?;
                Ok(rows.join(" "))
            };
            let ends = plan_of(
                "SELECT source, target FROM lex_relation WHERE about = ?1 \
                 AND type IN ('supersedes', 'corrects', 'updates_state')",
                &["about"],
            )?;
            assert!(ends.contains("lex_relation_by_type"), "{ends}");
            let supports = plan_of(
                "SELECT source FROM lex_relation WHERE about = ?1 AND target = ?2 \
                 AND type = 'supports'",
                &["about", "node"],
            )?;
            assert!(supports.contains("lex_relation_by_type"), "{supports}");
            Ok(())
        })
        .expect("sidecar reads");
}
