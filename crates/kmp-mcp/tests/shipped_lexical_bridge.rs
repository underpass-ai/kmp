//! The table every release publishes and `kmp-mcp setup` installs.
//!
//! `distribution/lexical-bridge/kmp-lexical-bridge.kmpb` is copied into the
//! release candidate byte for byte, so this is the place to prove that the
//! bytes a release will ship are ones this kernel reads, that the checksum
//! beside them is theirs, and that the pairs the documentation promises
//! actually bridge.

use std::path::PathBuf;

use kmp_proto_mapping::v1beta1::LexicalBridge;
use sha2::{Digest, Sha256};

fn shipped() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../distribution/lexical-bridge")
        .join("kmp-lexical-bridge.kmpb")
}

fn table() -> LexicalBridge {
    let bytes = std::fs::read(shipped()).expect("the shipped table is committed");
    LexicalBridge::from_bytes(&bytes).expect("the shipped table is one this kernel reads")
}

#[test]
fn the_shipped_table_is_one_this_kernel_reads() {
    let bridge = table();

    assert!(!bridge.is_silent());
    assert!(bridge.len() > 100_000, "{} words", bridge.len());
    assert_eq!(
        bridge.provenance(),
        "sentence-transformers/static-similarity-mrl-multilingual-v1"
    );
}

/// `setup` compares the digest a release publishes against the bytes it
/// downloads; the committed checksum is the one the release will publish.
#[test]
fn the_committed_checksum_is_the_tables_own() {
    let path = shipped();
    let bytes = std::fs::read(&path).expect("the shipped table is committed");
    let recorded = std::fs::read_to_string(path.with_extension("kmpb.sha256"))
        .expect("the checksum sits beside the table");

    let actual = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(recorded.split_whitespace().next(), Some(actual.as_str()));
    assert!(recorded.trim_end().ends_with("kmp-lexical-bridge.kmpb"));
}

/// The pairs the README cites as what the table does.
#[test]
fn the_documented_pairs_bridge_at_the_kernels_bar() {
    let bridge = table();

    for (spanish, english) in [
        ("valvula", "valve"),
        ("noche", "night"),
        ("fabrica", "factory"),
        ("cliente", "customer"),
        ("reunion", "meeting"),
    ] {
        let similarity = bridge
            .similarity(spanish, english)
            .unwrap_or_else(|| panic!("{spanish} and {english} are both in the shipped table"));
        assert!(
            similarity >= 0.45,
            "{spanish}\u{2248}{english} scores {similarity:.2}, below the 0.45 bar"
        );
    }
    assert!(
        bridge.similarity("valvula", "night").unwrap_or(1.0) < 0.45,
        "unrelated words do not bridge"
    );
}
