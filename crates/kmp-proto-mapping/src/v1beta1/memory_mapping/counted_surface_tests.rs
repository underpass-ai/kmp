//! A candidate read from its lexical sidecar row is the candidate the ranker
//! reads from its text: an indexed ask takes the row's counts instead of
//! tokenizing the candidate again, so the two must be equal for every
//! language and reading.
use kmp_domain::SearchSummary;
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::lexical_profile::LexicalProfile;
use super::lexical_row::LexicalRow;
use super::morphology::Morphology;

fn items() -> Vec<MemoryEvidence> {
    let mut summarized = MemoryEvidence {
        id: "entry:project:plant:entry:decision:valve-froze-b9e0944852682702".into(),
        text: "The reserve valves froze; corte 10 replaced them at 03:00 (#469, ADR 18).".into(),
        source: "operator log".into(),
        supports: vec!["project:plant:entry:fact:night-shift".into()],
        ..Default::default()
    };
    summarized
        .metadata
        .insert("shift".into(), "night crews Freezing c6.24".into());
    summarized.metadata.insert(
        SearchSummary::METADATA_KEY.into(),
        "The reserve valves froze during the night (#469).".into(),
    );
    let spanish = MemoryEvidence {
        id: "detail:evidence:project:plant:entry:observation:valvula-1:current".into(),
        text: "Las válvulas de reserva se congelaron durante la noche del turno 3.".into(),
        source: "parte de turno".into(),
        supports: vec![
            "project:plant:entry:observation:valvula-1".into(),
            "project:plant:entry:decision:cinta-2".into(),
        ],
        ..Default::default()
    };
    let bare = MemoryEvidence {
        id: "entry:project:plant:entry:note:kmp-469".into(),
        text: "kmp-469 0.7.0".into(),
        ..Default::default()
    };
    vec![summarized, spanish, bare]
}

#[test]
fn a_candidate_read_from_its_row_equals_the_candidate_read_from_its_text() {
    for language in [None, Some("en"), Some("es")] {
        let profile = LexicalProfile::new(language.map(str::to_string));
        for aliased in [false, true] {
            let context = AnswerRecallContext {
                morphology: Morphology::for_language(language),
                identifier_aliases: aliased,
                ..AnswerRecallContext::default()
            };
            for item in items() {
                let row = LexicalRow::read(&item, &profile);
                let read = AnswerCandidateTerms::from_evidence(&item, &context);
                let counted = AnswerCandidateTerms::from_counted(
                    &item,
                    &context,
                    row.counted_surface(aliased),
                );
                assert_eq!(
                    format!("{counted:?}"),
                    format!("{read:?}"),
                    "{language:?}, aliased {aliased}: {}",
                    item.id
                );
            }
        }
    }
}
