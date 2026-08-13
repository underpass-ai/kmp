use kmp_testkit::{LongMemEvalAdapterConfig, parse_longmemeval_dataset, prepare_longmemeval_items};

const MINIMAL_LONGMEMEVAL: &str = include_str!("fixtures/longmemeval_minimal.json");

#[test]
fn fixture_prepares_longmemeval_kmp_artifacts() {
    let dataset = parse_longmemeval_dataset(MINIMAL_LONGMEMEVAL).expect("fixture must parse");
    let (prepared, summary) =
        prepare_longmemeval_items(&dataset, &LongMemEvalAdapterConfig::default())
            .expect("fixture should adapt");

    assert_eq!(summary.dataset_items, 1);
    assert_eq!(summary.prepared_items, 1);
    assert_eq!(summary.sessions, 1);
    assert_eq!(summary.turns, 2);
    assert_eq!(summary.expected_evidence_turns, 1);
    assert_eq!(summary.relation_evidence_turns, 1);
    assert_eq!(prepared[0].about, "longmemeval:item:830ce83f");
    assert_eq!(
        prepared[0].expected.answer_turn_refs,
        vec!["turn:830ce83f:session-a:2"]
    );
    assert_eq!(
        prepared[0].ingest["memory"]["dimensions"][0]["metadata"]["abstention"],
        "false"
    );
    assert_eq!(
        prepared[0].ingest["memory"]["entries"][1]["metadata"]["has_answer"],
        "true"
    );
}
