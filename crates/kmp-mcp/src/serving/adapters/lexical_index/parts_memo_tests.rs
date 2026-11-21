//! The top-k rounds of one ask share their point reads ([`PartsMemo`]): a
//! later round reads from the store and the sidecar only what no earlier
//! round read, and its parts are exactly those a read from nothing gives.

use std::collections::BTreeSet;
use std::sync::Arc;

use kmp_domain::PortError;
use kmp_embedded::EmbeddedKernel;
use serde_json::json;

use super::indexed_parts::IndexedParts;
use super::parts_memo::PartsMemo;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use super::upkeep_tests::{ABOUT, call, memories, memory, shadowed, write};

/// What a round's parts say, comparable across reads.
fn parts_of(parts: Option<kmp_application::RecallParts>) -> String {
    let parts = parts.expect("the about has a root");
    format!("{:?} {:?}", parts.neighborhood, parts.details)
}

#[tokio::test]
async fn a_later_round_reads_only_what_no_earlier_round_read() {
    let directory = tempfile::tempdir().expect("store");
    let server = shadowed(directory.path());
    let notes = (0..12)
        .map(|index| {
            memory(
                &format!("valve-{index}"),
                &format!("The reserve valve {index} froze during the night shift."),
                &format!("Operator log {index} records the frozen valve."),
            )
        })
        .collect::<Vec<_>>();
    write(
        &server,
        memories("memo:1", json!({"component":["valve"]}), json!(notes)),
    )
    .await;
    // The first ask builds the about in the sidecar.
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"which valve froze?"}),
    )
    .await;
    let sidecar_path = directory
        .path()
        .join(super::lexical_sidecar::LEXICAL_INDEX_FILE);
    let sidecar = Arc::new(SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar"));
    let documents = sidecar
        .fingerprints(ABOUT, false)
        .expect("rows")
        .into_keys()
        .collect::<Vec<_>>();
    assert!(documents.len() >= 12, "{documents:?}");
    let first = documents[..documents.len() / 3]
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let every = documents.iter().cloned().collect::<BTreeSet<_>>();
    let kernel = EmbeddedKernel::open(directory.path()).expect("reader");
    let checked = kernel
        .store()
        .read_points(move |reads| {
            let read = |memo: &mut PartsMemo, candidates: &BTreeSet<String>| {
                IndexedParts::new(reads, &sidecar, ABOUT, memo)
                    .read(candidates)
                    .map(parts_of)
                    .map_err(PortError::Unavailable)
            };
            let mut shared = PartsMemo::default();
            let round_one = read(&mut shared, &first)?;
            let after_one = shared.misses;
            // The same candidates again read nothing new, and say the same.
            assert_eq!(read(&mut shared, &first)?, round_one);
            assert_eq!(shared.misses, after_one, "a repeated round read again");
            // A wider round equals a read from nothing, and reads less.
            let round_two = read(&mut shared, &every)?;
            let mut fresh = PartsMemo::default();
            assert_eq!(round_two, read(&mut fresh, &every)?);
            assert!(after_one > 0);
            assert!(
                shared.misses - after_one < fresh.misses,
                "the wider round read again what the first had read: {} new of {}",
                shared.misses - after_one,
                fresh.misses
            );
            // The last round reads through: the same parts, nothing kept
            // for a round that will not come.
            let mut through = PartsMemo {
                read_through: true,
                ..PartsMemo::default()
            };
            assert_eq!(read(&mut through, &every)?, round_two);
            assert!(through.nodes.is_empty() && through.edges.is_empty());
            assert!(through.details.is_empty());
            Ok(())
        })
        .await;
    checked.expect("parts read");
}
