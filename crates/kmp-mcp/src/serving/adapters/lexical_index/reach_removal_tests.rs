//! A candidate's reach after the log takes something away (P14).
//!
//! The log never removes a relation or a `supports` edge: it appends, and a
//! relation written again replaces its words (`upkeep_tests`). What it does
//! remove is a `contains_entry` edge, when a relabel takes a label off an
//! entry (an entry keeps one label at least). Reaches followed write by
//! write across such removals, around entries that carry relations and
//! evidence, must equal those of a sidecar built from nothing.

use std::path::Path;

use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

use kmp_embedded::EmbeddedKernel;

use super::lexical_sidecar::LEXICAL_INDEX_FILE;
use super::upkeep_tests::{
    ABOUT, AT, assert_same, call, dump, memories, memory, rebuilt, shadowed, write,
};

#[tokio::test]
async fn reaches_follow_labels_taken_off_entries_with_relations() {
    let directory = tempdir().expect("store");
    let scratch = tempdir().expect("scratch");
    let server = shadowed(directory.path());
    let first = write(
        &server,
        memories(
            "removal:1",
            json!({"component":["valve","manifold"],"task":["night-shift"]}),
            json!([
                memory(
                    "valve",
                    "The reserve valve froze during the night shift.",
                    "Operator log 14 records the frozen reserve valve."
                ),
                memory(
                    "crew",
                    "The night crew replaced the valve at 03:00.",
                    "Maintenance ticket M-201 closes the replacement."
                ),
                memory(
                    "glycol",
                    "Glycol was topped up in the manifold.",
                    "Fluid sheet F-3 lists the glycol batch."
                ),
            ]),
        ),
    )
    .await;
    let reference = |id: &str| first["local_refs"][id].as_str().expect("ref").to_string();
    let (valve, crew, glycol) = (reference("valve"), reference("crew"), reference("glycol"));
    // An explanatory relation and a support, each with words of its own.
    write(
        &server,
        json!({"about":ABOUT,"actor":"fixture","idempotency_key":"removal:links","observed_at":AT,
            "read_context":{"inspected_refs":[crew, valve, glycol]},
            "relations":[
                {"from":crew,"to":valve,"rel":"depends_on",
                    "why":"The crew waited for the thawing gear.",
                    "evidence":"Gear manifest G-4 arrived at 02:40."},
                {"from":glycol,"to":valve,"rel":"supports",
                    "why":"The glycol batch explains the freeze.",
                    "evidence":"Sheet F-3 shows a diluted batch."}]}),
    )
    .await;
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"why did the valve freeze?"}),
    )
    .await;
    let sidecar_path = directory.path().join(LEXICAL_INDEX_FILE);
    assert!(sidecar_path.exists(), "the first ask builds the sidecar");
    assert!(
        reaches_carry(&sidecar_path, b"thaw"),
        "the relation's words reach its ends"
    );

    // Labels taken off each end of both relations: one moved to another
    // label, one dropped where the entry keeps another.
    for (key, entry, add, remove) in [
        (
            "removal:valve",
            &valve,
            json!({"component":["boiler"]}),
            json!({"component":["valve"]}),
        ),
        (
            "removal:crew",
            &crew,
            json!({}),
            json!({"component":["manifold"]}),
        ),
        (
            "removal:glycol",
            &glycol,
            json!({"task":["day-shift"]}),
            json!({"task":["night-shift"]}),
        ),
    ] {
        call(
            &server,
            "kmp_relabel",
            json!({"about":ABOUT,"ref":entry,"actor":"fixture","observed_at":AT,
                "add":add,"remove":remove,"why":"The registry moved it.",
                "idempotency_key":key}),
        )
        .await;
    }
    // The next ask follows the relabels.
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"what did the crew wait for?"}),
    )
    .await;
    let followed = dump(&sidecar_path);
    let kernel = EmbeddedKernel::open(directory.path()).expect("reader");
    let built = rebuilt(&kernel, &scratch.path().join("built.sqlite3")).await;
    assert_same(&followed, &built);
    assert_eq!(followed["reach"], built["reach"]);
    assert!(
        reaches_carry(&sidecar_path, b"thaw"),
        "the relations still reach their ends"
    );
}

/// Whether some candidate's stored reach holds `word`.
fn reaches_carry(path: &Path, word: &[u8]) -> bool {
    let connection = Connection::open(path).expect("sidecar");
    let mut statement = connection
        .prepare("SELECT reach FROM lex_reach WHERE about = ?1")
        .expect("query");
    statement
        .query_map([ABOUT], |row| row.get::<_, Vec<u8>>(0))
        .expect("rows")
        .map(|reach| reach.expect("reach"))
        .any(|reach| reach.windows(word.len()).any(|window| window == word))
}
