//! Same-snapshot visual projection control: baseline|cache|verify SHAPE SAMPLES.
#[allow(dead_code)]
#[path = "../tests/support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::{VisualLevelOfDetail, VisualProjectionQuery};
use kmp_domain::*;
use serde_json::json;
use std::{sync::atomic::Ordering, time::Instant};
use support::{ABOUT, Reads, command, service};

fn query(lod: VisualLevelOfDetail) -> VisualProjectionQuery {
    VisualProjectionQuery {
        about: ABOUT.into(),
        from: "2026-09-01T00:00:00Z".into(),
        to: "2026-10-01T00:00:00Z".into(),
        axis: TemporalAxis::Observed,
        dimensions: DimensionSelection::all(),
        level_of_detail: lod,
        bin_count: 64,
        page_entries: 2048,
        cursor: None,
        depth: 8,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("verify");
    let shape = args.get(2).map(String::as_str).unwrap_or("small");
    let samples: usize = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(20);
    assert!(samples > 0 && matches!(mode, "baseline" | "cache" | "verify"));
    let (count, body_bytes, labels, degree) = match shape {
        "small" => (16, 128, 2, 1),
        "medium" => (256, 1024, 4, 2),
        "dense" => (2048, 256, 8, 8),
        "large_body" => (256, 16384, 4, 2),
        _ => panic!("unknown fixture"),
    };
    let dir = tempfile::tempdir()?;
    let store = EmbeddedKernelStore::open(dir.path())?;
    let mut reads = Reads::new(store.clone());
    reads.cache_revisions = mode != "baseline";
    let app = service(reads.clone(), true);
    let oracle = service(Reads::new(store.clone()), true);
    let mut seed = command(0, "benchmark");
    for label in 1..labels {
        let mut dimension = seed.memory.dimensions[0].clone();
        dimension.id = format!("task:label{label}");
        seed.memory.dimensions.push(dimension);
    }
    let mut prototype = seed.memory.entries[0].clone();
    for label in 1..labels {
        let mut coordinate = prototype.coordinates[0].clone();
        coordinate.scope_id = format!("task:label{label}");
        prototype.coordinates.push(coordinate);
    }
    seed.memory.entries = (0..count)
        .map(|i| {
            let mut entry = prototype.clone();
            entry.id = format!("{ABOUT}:entry:observation:n{i:05}");
            entry.text = format!("fixture {i}: {}", "x".repeat(body_bytes));
            entry
        })
        .collect();
    app.ingest(seed.clone()).await?;
    let mut edges = vec![];
    for i in 0..count {
        for offset in 1..=degree {
            edges.push(ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: seed.memory.entries[i].id.clone(),
                    target_node_id: seed.memory.entries[(i + offset) % count].id.clone(),
                    relation_type: "follows".into(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Procedural)
                        .with_rationale("Fixed synthetic traversal order.")
                        .with_evidence("Reproducible fixture.")
                        .with_observed_at("2026-09-10T10:00:00Z"),
                },
            )));
        }
    }
    store.apply_mutations(edges).await?;
    let mut output = vec![];
    for lod in [
        VisualLevelOfDetail::Atlas,
        VisualLevelOfDetail::Episode,
        VisualLevelOfDetail::Moment,
    ] {
        let q = query(lod);
        let start = Instant::now();
        let first = app.visual_projection(q.clone()).await?;
        let first_us = start.elapsed().as_micros();
        assert_eq!(first.page.total, count, "fixture entry coverage");
        assert_eq!(first.labels.len(), labels, "fixture label coverage");
        assert!(first.labels.iter().all(|label| label.in_range == count));
        if lod == VisualLevelOfDetail::Moment {
            assert!(
                first
                    .entries
                    .iter()
                    .all(|entry| entry.coordinates.len() == labels)
            );
            assert_eq!(
                first.relations.len(),
                count * degree,
                "fixture relation coverage"
            );
        }
        if mode == "verify" {
            let expected = oracle.visual_projection(q.clone()).await?;
            assert_eq!(first, expected);
            let graphs = reads.catalogue_reads.load(Ordering::SeqCst);
            let details = reads.detail_ids.lock().expect("counts").len();
            assert_eq!(app.visual_projection(q.clone()).await?, expected);
            assert_eq!(reads.catalogue_reads.load(Ordering::SeqCst), graphs);
            assert_eq!(reads.detail_ids.lock().expect("counts").len(), details);
            output.push(
                json!({"lod":lod,"equivalent":true,"entries":first.page.total,
                "relations":first.relations.len(),"hit_graph_reads":0,"hit_body_reads":0}),
            );
            continue;
        }
        let graphs = reads.catalogue_reads.load(Ordering::SeqCst);
        let details = reads.detail_ids.lock().expect("counts").len();
        let mut warm = vec![];
        let mut encoding = vec![];
        for _ in 0..samples {
            let start = Instant::now();
            let result = app.visual_projection(q.clone()).await?;
            warm.push(start.elapsed().as_micros());
            // Correctness and transport encoding are outside the application timing.
            assert_eq!(result, first);
            let encode = Instant::now();
            let _bytes = serde_json::to_vec(&result)?;
            encoding.push(encode.elapsed().as_micros());
        }
        let warm_graphs = reads.catalogue_reads.load(Ordering::SeqCst) - graphs;
        let warm_details = reads.detail_ids.lock().expect("counts").len() - details;
        let mut changed_views = vec![];
        for bins in 1..=5 {
            let mut changed = q.clone();
            changed.bin_count = bins;
            let start = Instant::now();
            let result = app.visual_projection(changed.clone()).await?;
            changed_views.push(start.elapsed().as_micros());
            if mode == "cache" {
                assert_eq!(result, oracle.visual_projection(changed).await?);
            }
        }
        let mut sorted = warm.clone();
        sorted.sort_unstable();
        output.push(json!({"lod":lod,"first_read_us":first_us,"warm_us":warm,
            "p50_us":sorted[sorted.len()/2],"p95_us":sorted[(sorted.len()*95).div_ceil(100)-1],
            "encoding_us":encoding,"changed_view_miss_us":changed_views,
            "warm_graph_reads":warm_graphs,"warm_body_refs_read":warm_details,
            "response_bytes":serde_json::to_vec(&first)?.len(),"entries":first.page.total,
            "relations":first.relations.len()}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"mode":mode,"shape":shape,
        "entries":count,"body_bytes_each":body_bytes,"labels_each":labels,"degree":degree,
        "results":output,"process_status":std::fs::read_to_string("/proc/self/status")?}))?
    );
    Ok(())
}
