//! Pins what a row reads: a change to the tokenizer, the stemmer, the alias
//! terms, the surface of a candidate or its judged expansions changes these
//! fingerprints, and a sidecar written before the change would then disagree
//! with the ranker.
//! When this fails, bump `INDEX_VERSION` (so every sidecar is built again)
//! and record the new fingerprints with it.

use std::collections::HashMap;

use kmp_domain::{SearchExpansions, SearchSummary};
use kmp_proto::v1beta1::MemoryEvidence;
use kmp_proto_mapping::v1beta1::{LexicalProfile, LexicalRow};

use super::lexical_maintainer::INDEX_VERSION;

fn fixtures() -> Vec<MemoryEvidence> {
    let mut metadata = HashMap::new();
    metadata.insert("shift".to_string(), "night crews Freezing".to_string());
    metadata.insert(
        "search_summary".to_string(),
        "The reserve valve froze during the night (#469).".to_string(),
    );
    vec![
        MemoryEvidence {
            id: "entry:project:plant:entry:decision:valve-froze-b9e0944852682702".to_string(),
            text: "The reserve valve froze; the valves were replaced at 03:00 (#469), corte 10."
                .to_string(),
            source: "operator log".to_string(),
            supports: vec!["project:plant:entry:fact:night-shift".to_string()],
            metadata,
            ..Default::default()
        },
        MemoryEvidence {
            id: "detail:evidence:project:plant:entry:fact:night-shift:current".to_string(),
            text: "La válvula de reserva se congeló durante la noche; ADR 18 fija el cambio."
                .to_string(),
            source: "parte de guardia".to_string(),
            supports: vec!["project:plant:entry:fact:night-shift".to_string()],
            ..Default::default()
        },
        expanded(),
    ]
}

/// A memory with judged search expansions (P15), bound to its text.
fn expanded() -> MemoryEvidence {
    let text = "The rollout slipped because the auditors had not signed off.";
    let mut metadata = HashMap::new();
    metadata.insert(
        SearchExpansions::METADATA_KEY.to_string(),
        "Why was the launch postponed?\n¿Por qué se retrasó el lanzamiento?".to_string(),
    );
    metadata.insert(
        SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY.to_string(),
        SearchSummary::source_fingerprint(text),
    );
    metadata.insert(
        SearchExpansions::JUDGED_BY_METADATA_KEY.to_string(),
        "jev-1.13.0 noul>=0.50".to_string(),
    );
    MemoryEvidence {
        id: "entry:project:plant:entry:decision:rollout-slipped-0123456789abcdef".to_string(),
        text: text.to_string(),
        source: "release notes".to_string(),
        supports: vec!["project:plant:entry:decision:rollout-slipped".to_string()],
        metadata,
        ..Default::default()
    }
}

fn fingerprint(language: Option<&str>) -> (u64, u64) {
    let profile = LexicalProfile::new(language.map(str::to_string));
    fixtures()
        .iter()
        .fold((0u64, 0u64), |(plain, aliased), item| {
            let row = LexicalRow::read(item, &profile);
            (
                plain.wrapping_mul(31).wrapping_add(row.fingerprint(false)),
                aliased.wrapping_mul(31).wrapping_add(row.fingerprint(true)),
            )
        })
}

#[test]
fn the_derivation_is_the_one_this_index_version_names() {
    let measured = [
        fingerprint(None),
        fingerprint(Some("english")),
        fingerprint(Some("spanish")),
    ];
    assert_eq!(
        (INDEX_VERSION, measured),
        ("lexical-index-2", GOLDEN),
        "the row derivation moved: bump INDEX_VERSION and record the new fingerprints"
    );
}

const GOLDEN: [(u64, u64); 3] = [
    (911_897_395_971_443_000, 4_370_273_265_505_948_218),
    (2_465_792_658_603_608_410, 3_987_275_543_019_347_018),
    (989_271_219_903_669_556, 13_049_686_741_475_258_028),
];
