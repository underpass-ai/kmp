//! Synthetic dimensional lookup and same-snapshot scheduling control (#775).
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::*;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};

fn node(id: &str, kind: &str, bytes: usize) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: id.into(),
        summary: "x".repeat(bytes),
        status: "ACTIVE".into(),
        labels: vec![],
        properties: BTreeMap::from([("dimension_kind".into(), "task".into())]),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, kind: &str, bytes: usize) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: kind.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_rationale("x".repeat(bytes)),
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let shape = args.get(1).map(String::as_str).unwrap_or("small");
    let samples: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(30);
    let (anchors, degree, body) = match shape {
        "small" => (4, 2, 128),
        "medium" => (256, 4, 256),
        "high_degree" => (256, 64, 2048),
        "large_body" => (64, 8, 32768),
        _ => panic!("unknown shape"),
    };
    let scratch = std::path::Path::new("tmp/performance-775");
    std::fs::create_dir_all(scratch)?;
    let dir = tempfile::tempdir_in(scratch)?;
    let opened = Instant::now();
    let store = EmbeddedKernelStore::open(dir.path())?;
    let open_us = opened.elapsed().as_micros();
    let mut mutations = vec![];
    let mut roots = vec![];
    let mut selected_labels = vec![];
    for i in 0..anchors {
        let root = format!("project:fixture-{i:04}");
        roots.push(root.clone());
        mutations.push(node(&root, "memory_anchor", body));
        for j in 0..4 {
            let value = if i % 8 == 0 && j == 0 {
                "selected".into()
            } else {
                format!("value-{i}-{j}")
            };
            let label = MemoryDimensionIdentity::new(&root, "task", &value)?.node_id();
            if value == "selected" {
                selected_labels.push(label.clone());
            }
            mutations.push(node(&label, "memory_dimension", body));
            mutations.push(edge(&root, &label, "has_dimension", body));
        }
        for j in 0..degree {
            let id = format!("{root}:entry:{j:04}");
            mutations.push(node(&id, "memory_entry", body));
            mutations.push(edge(&root, &id, "contains_entry", body));
        }
    }
    let started = Instant::now();
    store.apply_mutations(mutations).await?;
    let seed_us = started.elapsed().as_micros();
    let expected: Vec<_> = roots.iter().step_by(8).cloned().collect();
    let mut output = vec![];
    for sample in 0..=samples {
        let pin = Instant::now();
        let snapshot = store.read_snapshot().await?;
        let pin_us = pin.elapsed().as_micros();
        let start = Instant::now();
        let found = snapshot
            .list_memory_abouts_by_dimensions(&["selected".into()])
            .await?;
        let elapsed_us = start.elapsed().as_micros();
        assert_eq!(found, expected);
        output.push(json!({"sample":sample,"pin_us":pin_us,"lookup_us":elapsed_us}));
    }
    // Same pinned connection, real port work; preserve order after a bounded join.
    let mut schedules = vec![];
    for round in 0..6 {
        for width in if round % 2 == 0 { [1, 4] } else { [4, 1] } {
            let snapshot = store.read_snapshot().await?;
            let start = Instant::now();
            let mut found = vec![];
            for batch in roots
                .iter()
                .take(32)
                .cloned()
                .collect::<Vec<_>>()
                .chunks(width)
            {
                if width == 1 {
                    found.push(
                        snapshot
                            .load_neighborhood(&batch[0], 1)
                            .await?
                            .expect("root")
                            .root
                            .node_id,
                    );
                    continue;
                }
                let mut work = vec![];
                for root in batch {
                    let snapshot = snapshot.clone();
                    let root = root.clone();
                    work.push(tokio::spawn(async move {
                        snapshot.load_neighborhood(&root, 1).await
                    }));
                }
                for task in work {
                    found.push(task.await??.expect("root").root.node_id);
                }
            }
            assert_eq!(found, roots.iter().take(32).cloned().collect::<Vec<_>>());
            schedules.push(
                json!({"round":round,"width":width,"elapsed_us":start.elapsed().as_micros()}),
            );
        }
    }
    // Four operations use four pinned snapshots while one independent handle
    // commits whole-set type changes. Any proper snapshot sees all or none.
    let peer = EmbeddedKernelStore::open(dir.path())?;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(5));
    let mut readers = vec![];
    let mixed_start = Instant::now();
    for _ in 0..4 {
        let store = store.clone();
        let barrier = barrier.clone();
        let expected = expected.clone();
        readers.push(tokio::spawn(async move {
            barrier.wait().await;
            let mut times = vec![];
            for _ in 0..12 {
                let start = Instant::now();
                let snapshot = store.read_snapshot().await?;
                let found = snapshot
                    .list_memory_abouts_by_dimensions(&["selected".into()])
                    .await?;
                times.push(start.elapsed().as_micros());
                assert!(
                    found.is_empty() || found == expected,
                    "mixed snapshot: {found:?}"
                );
            }
            Ok::<_, PortError>(times)
        }));
    }
    barrier.wait().await;
    let mut write_times = vec![];
    for turn in 0..12 {
        let kind = if turn % 2 == 0 {
            "memory_entry"
        } else {
            "memory_dimension"
        };
        let changes = selected_labels
            .iter()
            .map(|id| node(id, kind, body))
            .collect();
        let start = Instant::now();
        peer.apply_mutations(changes).await?;
        write_times.push(start.elapsed().as_micros());
        tokio::task::yield_now().await;
    }
    let mut read_times = vec![];
    for reader in readers {
        read_times.extend(reader.await??);
    }
    let mixed = json!({"readers":4,"writers":1,"read_us":read_times,"write_us":write_times,
        "elapsed_us":mixed_start.elapsed().as_micros()});
    println!(
        "{}",
        json!({"shape":shape,"anchors":anchors,"degree":degree,"body_bytes":body,
        "samples":samples,"open_us":open_us,"seed_us":seed_us,"lookups":output,"schedules":schedules,"mixed":mixed})
    );
    Ok(())
}
