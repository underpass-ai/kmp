use kmp_testkit::{
    MemoryAgentBenchAdapterConfig, parse_memoryagentbench_dataset, prepare_memoryagentbench_items,
};

const MINIMAL_MEMORYAGENTBENCH: &str = include_str!("fixtures/memoryagentbench_minimal.jsonl");

#[test]
fn fixture_prepares_memoryagentbench_kmp_artifacts() {
    let dataset =
        parse_memoryagentbench_dataset(MINIMAL_MEMORYAGENTBENCH).expect("fixture must parse");
    let (prepared, summary) = prepare_memoryagentbench_items(
        &dataset,
        &MemoryAgentBenchAdapterConfig {
            split: "Conflict_Resolution".to_string(),
            ..MemoryAgentBenchAdapterConfig::default()
        },
    )
    .expect("fixture should adapt");

    assert_eq!(summary.dataset_items, 1);
    assert_eq!(summary.prepared_items, 1);
    assert_eq!(summary.questions, 2);
    assert_eq!(summary.ingest_events, 1);
    assert_eq!(summary.ask_events, 2);
    assert_eq!(summary.context_entries, 4);
    assert_eq!(summary.truncated_context_entries, 0);
    assert_eq!(
        prepared[0].about,
        "memoryagentbench:split:conflict_resolution:source:factconsolidation_mh_32k:item:checkout-final"
    );
    assert_eq!(prepared[0].ask_events[1].required_ingest_events, 1);
    assert_eq!(
        prepared[0].expected[1].available_ref_ids,
        prepared[0].replay.known_at_snapshots[1].available_ref_ids
    );
    assert_eq!(
        prepared[0].expected[0].available_ref_ids[3],
        "memoryagentbench:split:conflict_resolution:source:factconsolidation_mh_32k:item:checkout-final:context:fact:3"
    );
}
