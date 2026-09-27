//! Pins what a row reads: a change to the tokenizer, the stemmer, the alias
//! terms or the surface of a candidate changes these fingerprints, and a
//! sidecar written before the change would then disagree with the ranker.
//! When this fails, bump `INDEX_VERSION` (so every sidecar is built again)
//! and record the new fingerprints with it.

use std::collections::HashMap;

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
    ]
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
        ("lexical-index-1", GOLDEN),
        "the row derivation moved: bump INDEX_VERSION and record the new fingerprints"
    );
}

const GOLDEN: [(u64, u64); 3] = [
    (12_739_544_184_729_500_568, 3_330_204_529_574_070_870),
    (15_924_881_968_929_872_078, 2_882_724_332_246_181_854),
    (11_175_377_191_073_710_527, 853_410_165_099_505_991),
];
