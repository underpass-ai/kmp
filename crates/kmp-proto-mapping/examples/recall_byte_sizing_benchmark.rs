//! Informational #769 control for recall projection byte sizing.

use std::time::Instant;

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_proto_mapping::v1beta1::recall_projection::{ProjectionOutcome, project_recall_output};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn main() {
    let mode = std::env::args()
        .nth(1)
        .expect("mode: timing|allocation|parity");
    let shape = std::env::args().nth(2).expect("shape");
    let iterations = std::env::args()
        .nth(3)
        .map(|value| value.parse::<usize>().expect("iterations"))
        .unwrap_or(0);
    let (packet, arguments) = fixture(&shape);
    let estimator = Cl100kEstimator::new();

    match mode.as_str() {
        "timing" => {
            for _ in 0..3 {
                let _ = project(&packet, &arguments, &estimator);
            }
            let mut samples_us = Vec::with_capacity(iterations);
            let mut digest = String::new();
            for _ in 0..iterations {
                let started = Instant::now();
                let output = project(&packet, &arguments, &estimator);
                samples_us.push(started.elapsed().as_micros());
                digest = sha(&output);
            }
            println!(
                "{}",
                json!({"shape": shape, "samples_us": samples_us, "digest": digest})
            );
        }
        "allocation" => {
            let warm = project(&packet, &arguments, &estimator);
            let digest = sha(&warm);
            for _ in 0..iterations {
                let _ = project(&packet, &arguments, &estimator);
            }
            println!(
                "{}",
                json!({"shape": shape, "iterations": iterations, "digest": digest})
            );
        }
        "parity" => println!("{}", page_sequence(&packet, arguments, &estimator)),
        _ => panic!("mode must be timing, allocation, or parity"),
    }
}

fn project(packet: &Value, arguments: &Value, estimator: &Cl100kEstimator) -> Value {
    match project_recall_output(packet.clone(), arguments, 2_400, estimator).expect("projection") {
        ProjectionOutcome::Projected(value) => value,
        ProjectionOutcome::CoreTooLarge => panic!("fixture core should fit"),
    }
}

fn page_sequence(packet: &Value, mut arguments: Value, estimator: &Cl100kEstimator) -> Value {
    let mut page_hashes = Vec::new();
    let mut page_bytes = Vec::new();
    let mut returned = Vec::new();
    for _ in 0..128 {
        let output = project(packet, &arguments, estimator);
        let serialized = serde_json::to_vec(&output).expect("output JSON");
        page_hashes.push(format!("{:x}", Sha256::digest(&serialized)));
        page_bytes.push(serialized.len());
        returned.push(output["projection"]["page"]["returned"].clone());
        if output["projection"]["page"]["has_more"] == false
            && output["projection"]["core_text_shortened"] == false
        {
            return json!({
                "page_hashes": page_hashes,
                "page_bytes": page_bytes,
                "returned": returned,
                "final_digest": sha(&output)
            });
        }
        arguments = output["projection"]["next_action"]["arguments"].clone();
    }
    panic!("fixture continuation did not finish");
}

fn sha(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("output JSON"))
    )
}

fn fixture(shape: &str) -> (Value, Value) {
    let (path_count, evidence_count, text_bytes, max_bytes, page_entries): (
        usize,
        usize,
        usize,
        usize,
        usize,
    ) = match shape {
        "empty" => (0, 0, 0, 4_000, 64),
        "small" => (8, 4, 64, 8_000, 64),
        "medium" => (256, 64, 256, 24_000, 64),
        "shortened_core" => (128, 32, 128, 4_000, 32),
        "oversized_item" => (16, 4, 24_000, 10_000, 1),
        _ => panic!("unknown shape"),
    };
    let escaped = "escaped \"quote\" \\ slash\n tab\t é界🚀";
    let evidence = (0..evidence_count)
        .map(|index| {
            json!({
                "id": format!("evidence:{index}"),
                "supports": [format!("claim:{index}")],
                "text": format!("{index}:{}", escaped.repeat(text_bytes.div_ceil(escaped.len()))),
                "source": format!("source:{index}")
            })
        })
        .collect::<Vec<_>>();
    let path = (0..path_count)
        .map(|index| {
            json!({
                "from": format!("node:{index:04}"),
                "to": format!("claim:{}", index % evidence_count.max(1)),
                "rel": if index % 2 == 0 { "depends_on" } else { "supports" },
                "class": if index % 2 == 0 { "causal" } else { "evidential" },
                "why": format!("{escaped}:{index}")
            })
        })
        .collect::<Vec<_>>();
    let mut answer = format!("Answer {escaped}");
    if shape == "shortened_core" {
        answer = escaped.repeat(2_000);
    }
    let packet = json!({
        "summary": format!("Summary {escaped}"),
        "answer": answer,
        "because": [],
        "proof": {
            "path": path,
            "evidence": evidence,
            "conflicts": [], "superseded": [], "missing": [],
            "frontier_size": 0, "matched_terms": ["é", "界", "🚀"],
            "matched_relations": [], "confidence": "high"
        },
        "warnings": []
    });
    let arguments = json!({
        "about": "project:kmp",
        "question": format!("Question {escaped}"),
        "budget": {"max_bytes": max_bytes, "detail": "full"},
        "page": {"entries": page_entries}
    });
    (packet, arguments)
}
