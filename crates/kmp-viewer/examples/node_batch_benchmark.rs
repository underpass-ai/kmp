//! Synthetic framing benchmark: baseline|batch|baseline-http|batch-http|verify [samples].
#[path = "support/framing_http.rs"]
mod framing_http;
use kmp_application::InspectMemoryQuery;
use kmp_domain::*;
use kmp_embedded::EmbeddedKernel;
use kmp_viewer::views::node_batch_view;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};

const ABOUT: &str = "project:node-bench";
fn reference(i: usize) -> String {
    format!("{ABOUT}:entry:observation:n{i:03}")
}
fn mutations(
    count: usize,
    body_bytes: usize,
    labels: usize,
    sources: usize,
) -> Vec<ProjectionMutation> {
    let mut mutations = vec![];
    for (id, kind) in (0..count)
        .map(|i| (reference(i), "observation"))
        .chain((0..sources).map(|i| (format!("evidence:{ABOUT}:s{i:03}"), "memory_evidence")))
    {
        mutations.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.clone(),
            node_kind: kind.into(),
            title: id.clone(),
            summary: format!("Selected metadata {id}"),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: BTreeMap::from([("memory_about".into(), ABOUT.into())]),
            provenance: None,
        }));
        mutations.push(ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: id,
            detail: "x".repeat(body_bytes),
            content_hash: "fixture-content".into(),
            revision: 1,
        }));
    }
    for i in 0..count {
        for label in 0..labels {
            let scope = MemoryDimensionIdentity::new(ABOUT, "task", format!("label{label:03}"))
                .expect("label")
                .node_id();
            mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: scope.clone(),
                    target_node_id: reference(i),
                    relation_type: "contains_entry".into(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_dimension("task")
                        .with_scope_id(scope)
                        .with_observed_at("2026-09-13T10:00:00.123456789Z")
                        .with_occurred_at("2026-09-12T10:00:00Z"),
                },
            )));
        }
        for source in 0..sources {
            mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: format!("evidence:{ABOUT}:s{source:03}"),
                    target_node_id: reference(i),
                    relation_type: "supports".into(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                        .with_rationale("The synthetic source reports this observation.")
                        .with_evidence("A fixed fixture, not personal memory.")
                        .with_observed_at("2026-09-13T10:00:00Z"),
                },
            )));
        }
    }
    mutations
}
fn query(reference: String) -> InspectMemoryQuery {
    InspectMemoryQuery {
        about: ABOUT.into(),
        ref_id: reference,
        include_details: true,
        include_incoming: true,
        include_outgoing: true,
        include_raw: true,
        expect_revision: None,
    }
}
async fn measure(
    kernel: &EmbeddedKernel,
    mode: &str,
    refs: &[String],
) -> Result<(u128, usize), Box<dyn std::error::Error>> {
    let now = Instant::now();
    let bytes = if mode == "baseline" {
        let mut bytes = 0;
        for reference in refs {
            let inspected = kernel.service().inspect(query(reference.clone())).await?;
            bytes += serde_json::to_vec(&kmp_viewer::views::node_inspect_view(&inspected))?.len();
        }
        bytes
    } else {
        let read = kernel
            .service()
            .read_nodes(MemoryNodesRequest {
                expect_snapshot: None,
                about: ABOUT.into(),
                refs: refs.to_vec(),
                max_edges: 32768,
            })
            .await?;
        assert!(read.stop.is_none());
        serde_json::to_vec(&node_batch_view(&read))?.len()
    };
    Ok((now.elapsed().as_micros(), bytes))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "batch".into());
    assert!(matches!(
        mode.as_str(),
        "baseline" | "batch" | "baseline-http" | "batch-http" | "verify"
    ));
    let samples = std::env::args()
        .nth(2)
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(20_usize);
    assert!(samples > 0);
    let mut results = vec![];
    for (shape, count, body_bytes, labels, sources) in [
        ("small", 1, 128, 2, 1),
        ("medium", 8, 4096, 4, 4),
        ("high_degree", 64, 256, 32, 64),
        ("large_body", 8, 262144, 4, 8),
    ] {
        let dir = tempfile::tempdir()?;
        let kernel = EmbeddedKernel::open(dir.path())?;
        kernel
            .store()
            .apply_mutations(mutations(count, body_bytes, labels, sources))
            .await?;
        let refs = (0..count).map(reference).collect::<Vec<_>>();
        if mode == "verify" {
            let batch = kernel
                .service()
                .read_nodes(MemoryNodesRequest {
                    expect_snapshot: None,
                    about: ABOUT.into(),
                    refs: refs.clone(),
                    max_edges: 32768,
                })
                .await?;
            assert!(batch.stop.is_none());
            assert_eq!(batch.nodes.len(), refs.len());
            for node in batch.nodes {
                let inspected = kernel
                    .service()
                    .inspect(query(node.node.node_id.clone()))
                    .await?;
                let left = &node.node;
                let right = &inspected.detail.node;
                assert_eq!(
                    (
                        &left.node_id,
                        &left.node_kind,
                        &left.title,
                        &left.summary,
                        &left.status,
                        &left.labels,
                        &left.properties
                    ),
                    (
                        &right.node_id,
                        &right.node_kind,
                        &right.title,
                        &right.summary,
                        &right.status,
                        &right.labels,
                        &right.properties
                    )
                );
                assert_eq!(node.coordinates, inspected.raw_coordinates);
                assert!(node.coordinates_complete);
            }
            results.push(json!({"shape":shape,"equivalent":true}));
            continue;
        }
        let http = if mode.ends_with("-http") {
            Some(framing_http::FramingHttp::open(&kernel).await?)
        } else {
            None
        };
        let run = || async {
            if let Some(http) = &http {
                http.measure(mode.starts_with("baseline"), ABOUT, &refs)
                    .await
            } else {
                measure(&kernel, &mode, &refs).await
            }
        };
        let first = run().await?;
        let mut warm = vec![];
        for _ in 0..samples {
            warm.push(run().await?);
        }
        let mut reopened = vec![];
        for _ in 0..5 {
            let start = Instant::now();
            let reopened_kernel = EmbeddedKernel::open(dir.path())?;
            let open_us = start.elapsed().as_micros();
            let (read_us, bytes) = if mode.ends_with("-http") {
                framing_http::FramingHttp::open(&reopened_kernel)
                    .await?
                    .measure(mode.starts_with("baseline"), ABOUT, &refs)
                    .await?
            } else {
                measure(&reopened_kernel, &mode, &refs).await?
            };
            reopened.push(json!({"open_us":open_us,"read_us":read_us,"bytes":bytes}));
        }
        let mut sorted = warm.iter().map(|(us, _)| *us).collect::<Vec<_>>();
        sorted.sort();
        results.push(json!({"shape":shape,"refs":count,"body_bytes_each":body_bytes,"labels_each":labels,"shared_sources":sources,"mode":mode,"first_read_us":first.0,"warm_us":warm.iter().map(|(us,_)|us).collect::<Vec<_>>(),"p50_us":sorted[sorted.len()/2],"p95_us":sorted[(sorted.len()*95).div_ceil(100).saturating_sub(1)],"response_bytes":first.1,"reopened_same_process_os_cache_uncontrolled":reopened,"transport_trips":if mode.starts_with("baseline"){count}else{1}}));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
