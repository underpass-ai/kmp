use kmp_testkit::{MemoryArenaAdapterConfig, parse_memoryarena_dataset, prepare_memoryarena_items};

const MINIMAL_MEMORYARENA: &str = include_str!("fixtures/memoryarena_minimal.jsonl");

#[test]
fn fixture_prepares_memoryarena_kmp_artifacts() {
    let dataset = parse_memoryarena_dataset(MINIMAL_MEMORYARENA).expect("fixture must parse");
    let (prepared, summary) = prepare_memoryarena_items(
        &dataset,
        &MemoryArenaAdapterConfig {
            task_type: "progressive_search".to_string(),
            ..MemoryArenaAdapterConfig::default()
        },
    )
    .expect("fixture should adapt");

    assert_eq!(summary.dataset_items, 1);
    assert_eq!(summary.prepared_tasks, 1);
    assert_eq!(summary.subtasks, 2);
    assert_eq!(summary.ingest_events, 5);
    assert_eq!(summary.ask_events, 2);
    assert_eq!(summary.background_entries, 1);
    assert_eq!(
        prepared[0].about,
        "memoryarena:task_type:progressive_search:task:7"
    );
    assert_eq!(prepared[0].ask_events[1].required_ingest_events, 4);
    assert_eq!(
        prepared[0].expected[1].current_question_ref,
        "memoryarena:task_type:progressive_search:task:7:subtask:2:question"
    );
    assert_eq!(
        prepared[0].replay.known_at_snapshots[1].available_ref_ids,
        prepared[0].expected[1].available_ref_ids
    );
}
