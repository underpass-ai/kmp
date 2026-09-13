//! Synthetic scoped-neighborhood control for graph adjacency reads (#767).

use std::collections::BTreeMap;
use std::time::Instant;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, MemoryDimensionIdentity, NeighborhoodRequest, NodeProjection,
    NodeRelationProjection, ProjectionMutation, ProjectionWriter, RelationExplanation,
    RelationSemanticClass,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn node(id: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: id.into(),
        summary: format!("Synthetic graph fixture node {id}"),
        status: "ACTIVE".into(),
        labels: Vec::new(),
        properties: BTreeMap::new(),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, sequence: usize, explanation_bytes: usize) -> ProjectionMutation {
    let prefix = format!("edge {sequence} from {from} to {to}: ");
    let rationale = if prefix.len() >= explanation_bytes {
        prefix
    } else {
        let padding = explanation_bytes - prefix.len();
        prefix + &"x".repeat(padding)
    };
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: format!("related_{:02}", sequence % 7),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_rationale(rationale)
            .with_evidence(format!("fixture evidence {sequence}")),
    }))
}

fn shape(name: &str) -> (usize, usize, usize, usize) {
    match name {
        // selected entries, retained links per entry, discarded dimensions per entry, rationale
        "small" => (8, 1, 1, 128),
        "medium" => (128, 2, 4, 1024),
        "high_degree" => (256, 8, 16, 256),
        "large_payload" => (64, 2, 8, 32 * 1024),
        _ => panic!("unknown shape"),
    }
}

fn digest<T: std::fmt::Debug>(value: &T) -> String {
    format!("{:x}", Sha256::digest(format!("{value:?}").as_bytes()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let shape_name = args.get(1).map(String::as_str).unwrap_or("small");
    let samples: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(30);
    let (entry_count, retained_degree, discarded_degree, explanation_bytes) = shape(shape_name);
    let about = "project:graph-adjacency-control";
    let selected_dimension = MemoryDimensionIdentity::new(about, "task", "selected")?.node_id();
    let scratch = std::path::Path::new("tmp/performance-767");
    std::fs::create_dir_all(scratch)?;
    let dir = tempfile::tempdir_in(scratch)?;
    let store = EmbeddedKernelStore::open(dir.path())?;
    let mut mutations = vec![
        node(about, "memory_anchor"),
        node(&selected_dimension, "memory_dimension"),
    ];
    let mut sequence = 0usize;
    mutations.push(edge(
        about,
        &selected_dimension,
        sequence,
        explanation_bytes,
    ));
    sequence += 1;

    let entries: Vec<_> = (0..entry_count)
        .map(|index| format!("{about}:entry:observation:n{index:05}"))
        .collect();
    for entry in &entries {
        mutations.push(node(entry, "memory_entry"));
        mutations.push(edge(
            &selected_dimension,
            entry,
            sequence,
            explanation_bytes,
        ));
        sequence += 1;
    }
    for (index, source) in entries.iter().enumerate() {
        for offset in 1..=retained_degree {
            let target = &entries[(index + offset) % entries.len()];
            mutations.push(edge(source, target, sequence, explanation_bytes));
            sequence += 1;
        }
        for noise in 0..discarded_degree {
            let target = MemoryDimensionIdentity::new(
                about,
                "topic",
                format!("discarded-{index:05}-{noise:03}"),
            )?
            .node_id();
            mutations.push(node(&target, "memory_dimension"));
            mutations.push(edge(source, &target, sequence, explanation_bytes));
            sequence += 1;
        }
    }
    store.apply_mutations(mutations).await?;

    let request = NeighborhoodRequest::new(about, 3).with_scopes([selected_dimension]);
    let mut reads = Vec::with_capacity(samples + 1);
    let mut expected_digest = None;
    let mut relation_count = 0usize;
    let mut neighbor_count = 0usize;
    for sample in 0..=samples {
        let started = Instant::now();
        let result = store
            .load_scoped_neighborhood(&request)
            .await?
            .expect("fixture root");
        let elapsed_us = started.elapsed().as_micros();
        let result_digest = digest(&result);
        if let Some(expected) = &expected_digest {
            assert_eq!(&result_digest, expected, "repeat result changed");
        } else {
            expected_digest = Some(result_digest);
            relation_count = result.relations.len();
            neighbor_count = result.neighbors.len();
        }
        reads.push(json!({"sample": sample, "elapsed_us": elapsed_us}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "shape": shape_name,
            "samples": samples,
            "selected_entries": entry_count,
            "retained_degree": retained_degree,
            "discarded_degree": discarded_degree,
            "explanation_bytes": explanation_bytes,
            "neighbors": neighbor_count,
            "relations": relation_count,
            "result_digest": expected_digest.expect("one read"),
            "reads": reads,
            "process_status": std::fs::read_to_string("/proc/self/status")?,
        }))?
    );
    Ok(())
}
