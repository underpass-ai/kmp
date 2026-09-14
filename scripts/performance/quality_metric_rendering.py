#!/usr/bin/env python3
"""Measure quality-metric cost separately from production bundle rendering."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


RUNNER = r'''use kmp_application::{
    render_graph_bundle_with_options, ContextRenderOptions,
    queries::cl100k_estimator::Cl100kEstimator,
};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleQualityMetrics, BundleRelationship, CaseId,
    KmpBundle, RelationExplanation, RelationSemanticClass, Role,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Instant;

fn node(id: &str, kind: &str, title: &str, summary: &str) -> BundleNode {
    BundleNode::new(
        id,
        kind,
        title,
        summary,
        "ACTIVE",
        vec![],
        BTreeMap::new(),
    )
}

fn shape_bundle(shape: &str) -> KmpBundle {
    let (neighbors, details, relations, payload) = match shape {
        "small" => (4, 2, 3, "Short ASCII payload.".to_string()),
        "unicode" => (
            16,
            8,
            20,
            r#"Unicode 😀 café — escaped "quotes", backslash \\ and JSON \\n+line."#.to_string(),
        ),
        "large_payload" => (32, 32, 64, "payload-".repeat(512)),
        "relations" => (128, 16, 512, "Relation-heavy payload.".to_string()),
        other => panic!("unknown shape {other}"),
    };
    let root_id = format!("case:{shape}");
    let root = node(&root_id, "case", "Root", &format!("Root summary {payload}"));
    let neighbor_nodes = (0..neighbors)
        .map(|index| {
            let id = format!("node:{shape}:{index}");
            node(
                &id,
                if index % 3 == 0 { "decision" } else { "claim" },
                &format!("Title {index} {payload}"),
                &format!("Summary {index} {payload}"),
            )
        })
        .collect::<Vec<_>>();
    let relationships = (0..relations)
        .map(|index| {
            let target = format!("node:{shape}:{}", index % neighbors);
            let class = if index % 4 == 0 {
                RelationSemanticClass::Causal
            } else {
                RelationSemanticClass::Structural
            };
            BundleRelationship::new(
                &root_id,
                &target,
                if index % 2 == 0 { "RELATES_TO" } else { "SUPPORTS" },
                RelationExplanation::new(class)
                    .with_rationale(format!("Rationale {index} {payload}"))
                    .with_decision_id(format!("decision-{index}")),
            )
        })
        .collect::<Vec<_>>();
    let node_details = (0..details)
        .map(|index| {
            let id = if index == 0 {
                root_id.clone()
            } else {
                format!("node:{shape}:{}", index % neighbors)
            };
            BundleNodeDetail::new(&id, &format!("Detail {index} {payload}"), format!("hash-{index}"), index as u64)
        })
        .collect::<Vec<_>>();
    KmpBundle::new(
        CaseId::new(format!("case:{shape}")).expect("case id"),
        Role::new("memory").expect("role"),
        root,
        neighbor_nodes,
        relationships,
        node_details,
        BundleMetadata::initial("0.18.3"),
    )
    .expect("fixture bundle")
}

fn render(bundle: &KmpBundle) -> kmp_application::RenderedContext {
    render_graph_bundle_with_options(
        bundle,
        &ContextRenderOptions {
            token_budget: Some(512),
            ..Default::default()
        },
    )
}

fn metric(bundle: &KmpBundle, rendered_tokens: u32) -> BundleQualityMetrics {
    BundleQualityMetrics::compute(bundle, rendered_tokens, Cl100kEstimator::shared())
}

fn fingerprint<T: Debug>(value: &T) -> String {
    format!("{:x}", Sha256::digest(format!("{value:?}").as_bytes()))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).expect("mode");
    let shape = args.get(2).expect("shape");
    let warm_samples: usize = args.get(3).expect("warm samples").parse().expect("samples");
    let bundle = shape_bundle(shape);
    let mut result = serde_json::json!({
        "mode": mode,
        "shape": shape,
        "warm_samples": warm_samples,
        "bundle": {
            "neighbors": bundle.neighbor_nodes().len(),
            "relationships": bundle.relationships().len(),
            "details": bundle.node_details().len(),
        },
    });
    if mode == "render" {
        let first_started = Instant::now();
        let first = render(&bundle);
        let first_ms = first_started.elapsed().as_secs_f64() * 1000.0;
        let rendered_tokens = first.token_count;
        let rendered_bytes = first.content.len();
        let mut warm_ms = Vec::with_capacity(warm_samples);
        for _ in 0..warm_samples {
            let started = Instant::now();
            let rendered = render(&bundle);
            warm_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(rendered, first);
        }
        result["bundle"]["rendered_bytes"] = json!(rendered_bytes);
        result["bundle"]["rendered_tokens"] = json!(rendered_tokens);
        result["render_fingerprint"] = json!(fingerprint(&first));
        result["first_ms"] = json!(first_ms);
        result["warm_ms"] = json!(warm_ms);
    } else if mode == "metric" {
        let rendered_tokens: u32 = args.get(4).expect("rendered tokens").parse().expect("tokens");
        let first_started = Instant::now();
        let first = metric(&bundle, rendered_tokens);
        let first_ms = first_started.elapsed().as_secs_f64() * 1000.0;
        let mut warm_ms = Vec::with_capacity(warm_samples);
        for _ in 0..warm_samples {
            let started = Instant::now();
            let quality = metric(&bundle, rendered_tokens);
            warm_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(quality, first);
        }
        result["quality"] = json!({
            "raw_equivalent_tokens": first.raw_equivalent_tokens(),
            "compression_ratio": first.compression_ratio(),
            "causal_density": first.causal_density(),
            "noise_ratio": first.noise_ratio(),
            "detail_coverage": first.detail_coverage(),
        });
        result["rendered_tokens"] = json!(rendered_tokens);
        result["first_ms"] = json!(first_ms);
        result["warm_ms"] = json!(warm_ms);
    } else {
        panic!("unknown mode {mode}");
    }
    println!("{}", result);
}
'''


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def command_output(*command: str) -> str:
    return subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()


def run_phase(binary: Path, mode: str, shape: str, warm_samples: int, tokens: int | None,
              counter: Path | None = None) -> tuple[dict, dict | None]:
    command = [str(binary), mode, shape, str(warm_samples)]
    if tokens is not None:
        command.append(str(tokens))
    environment = dict(os.environ)
    if counter is not None:
        environment["LD_PRELOAD"] = str(counter)
    completed = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True, env=environment)
    result = json.loads(completed.stdout)
    if counter is None:
        if completed.stderr:
            raise RuntimeError(f"unexpected stderr in uninstrumented {mode}/{shape}: {completed.stderr}")
        return result, None
    counters = [json.loads(line) for line in completed.stderr.splitlines()
                if line.startswith('{"native_allocation_control":true,')]
    if len(counters) != 1:
        raise RuntimeError(f"missing allocation counter for {mode}/{shape}: {completed.stderr}")
    return result, counters[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--warm-samples", type=int, default=21)
    parser.add_argument("--allocation-warm-samples", type=int, default=5)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--runner-binary", type=Path)
    args = parser.parse_args()
    args.scratch.mkdir(parents=True, exist_ok=False)
    source = ROOT / "crates/kmp-application/src/bin/quality-metric-rendering-runner.rs"
    if args.runner_binary:
        binary = args.runner_binary.resolve()
    else:
        if source.exists():
            raise SystemExit(f"refusing to overwrite {source}")
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(RUNNER, encoding="utf-8")
        try:
            subprocess.run(
                ["cargo", "build", "--locked", "-p", "kmp-application", "--bin", "quality-metric-rendering-runner"],
                cwd=ROOT,
                check=True,
            )
        finally:
            source.unlink(missing_ok=True)
        binary = ROOT / "target/debug/quality-metric-rendering-runner"
    if args.build_only:
        print(binary)
        return 0

    counter = args.scratch / "native_allocation_counter.so"
    subprocess.run(
        ["cc", "-shared", "-fPIC", "-O2", "-Wall", "-Wextra", "-Werror", "-o", str(counter),
         str(ROOT / "scripts/performance/native_allocation_counter.c")],
        cwd=ROOT,
        check=True,
    )
    rows = []
    for shape in ("small", "unicode", "large_payload", "relations"):
        render, _ = run_phase(binary, "render", shape, args.warm_samples, None)
        tokens = render["bundle"]["rendered_tokens"]
        metric, _ = run_phase(binary, "metric", shape, args.warm_samples, tokens)
        _, render_allocations = run_phase(binary, "render", shape, args.allocation_warm_samples, None, counter)
        _, metric_allocations = run_phase(binary, "metric", shape, args.allocation_warm_samples, tokens, counter)
        rows.append({
            "shape": shape,
            "bundle": render["bundle"],
            "quality": metric["quality"],
            "render": render,
            "metric": metric,
            "allocation_control": {
                "scope": "separate process per phase; startup, bundle construction and all warm calls included",
                "metric": "Linux/glibc LD_PRELOAD allocation requests and cumulative requested bytes; not live heap",
                "render": render_allocations,
                "metric_phase": metric_allocations,
            },
        })
    cpuinfo = Path("/proc/cpuinfo")
    raw_cpuinfo = cpuinfo.read_bytes() if cpuinfo.exists() else platform.platform().encode()
    model = next(
        (line.split(":", 1)[1].strip() for line in raw_cpuinfo.decode(errors="replace").splitlines()
         if line.lower().startswith(("model name", "hardware", "cpu part")) and ":" in line),
        platform.processor() or "unknown",
    )
    result = {
        "issue": 768,
        "runner": str(Path(__file__).relative_to(ROOT)),
        "runner_sha256": digest(Path(__file__)),
        "runner_binary_sha256": digest(binary),
        "git_commit": command_output("git", "rev-parse", "HEAD"),
        "rustc": command_output("rustc", "-Vv"),
        "cargo_profile": "dev, workspace .cargo/config.toml",
        "host": {
            "system": platform.platform(),
            "machine": platform.machine(),
            "cpu_model": model,
            "hardware_sha256": hashlib.sha256(raw_cpuinfo).hexdigest(),
        },
        "method": {
            "render": "render_graph_bundle_with_options with token_budget=512",
            "metric": "BundleQualityMetrics::compute on the same bundle and rendered token count",
            "allocation_counter": "separate Linux/glibc LD_PRELOAD process controls; startup and bundle construction included, deallocations reported separately; do not interpret instrumented times",
            "warm_samples": args.warm_samples,
            "allocation_warm_samples": args.allocation_warm_samples,
            "candidate": "paired baseline/candidate attribution; allocation controls are process-scoped",
        },
        "source_hashes": {
            path: digest(ROOT / path)
            for path in (
                "crates/kmp-domain/src/value_objects/bundle_quality_metrics.rs",
                "crates/kmp-application/src/queries/render_graph_bundle.rs",
                "crates/kmp-application/src/queries/cl100k_estimator.rs",
                "crates/kmp-proto-mapping/src/v1beta1/recall_projection/typed_recall.rs",
                "scripts/performance/native_allocation_counter.c",
            )
        },
        "shapes": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
