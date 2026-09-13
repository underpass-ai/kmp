#!/usr/bin/env python3
"""Measure exact store-lease lookup with synthetic directory cardinalities.

The temporary Rust runner calls the public lease adapter directly. It creates
0, 1,000, and 10,000 unrelated empty lock files, then measures one first-use
claim separately from repeated claims for the same canonical store. The
synthetic directory is always under the caller-provided scratch directory.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


RUNNER = r'''use kmp_mcp::lifecycle::StoreSessionLease;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn directory_bytes(path: &Path) -> u64 {
    fs::metadata(path).expect("lease directory metadata").len()
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let scratch = PathBuf::from(&args[1]);
    let stale: usize = args[2].parse().expect("stale count");
    let warm_samples: usize = args[3].parse().expect("warm samples");
    let data_home = scratch.join("data");
    let store = scratch.join("project/.kernel");
    fs::create_dir_all(store.join("store")).expect("store");
    fs::write(store.join("FORMAT_VERSION"), "2").expect("format");
    fs::write(store.join("store/kernel.sqlite3"), [0_u8; 8]).expect("sqlite");
    let leases = data_home.join("kmp/store-leases");
    fs::create_dir_all(&leases).expect("leases");
    for index in 0..stale {
        fs::write(leases.join(format!("stale-{index:05}.lock")), []).expect("stale lease");
    }

    let cold_start = Instant::now();
    let cold = StoreSessionLease::acquire(&data_home, &store).expect("cold lease");
    let cold_ns = cold_start.elapsed().as_nanos();
    drop(cold);

    let warm_start = Instant::now();
    let mut warm_ns = Vec::new();
    for _ in 0..warm_samples {
        let start = Instant::now();
        let lease = StoreSessionLease::acquire(&data_home, &store).expect("warm lease");
        warm_ns.push(start.elapsed().as_nanos());
        drop(lease);
    }
    let warm_total_ns = warm_start.elapsed().as_nanos();
    let file_count = fs::read_dir(&leases).expect("read leases").count();
    println!("{}", serde_json::json!({
        "stale_files": stale,
        "file_count_after": file_count,
        "directory_metadata_bytes": directory_bytes(&leases),
        "cold_acquire_ns": cold_ns,
        "warm_acquire_ns": warm_ns,
        "warm_total_ns": warm_total_ns,
    }));
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


def hardware_fingerprint() -> tuple[str, str]:
    cpuinfo = Path("/proc/cpuinfo")
    raw = cpuinfo.read_bytes() if cpuinfo.exists() else platform.platform().encode()
    model = next(
        (line.split(":", 1)[1].strip() for line in raw.decode(errors="replace").splitlines()
         if line.lower().startswith(("model name", "hardware", "cpu part")) and ":" in line),
        platform.processor() or "unknown",
    )
    return model, hashlib.sha256(raw).hexdigest()


def percentile(values: list[int], fraction: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1))
    return ordered[index] / 1_000_000


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--warm-samples", type=int, default=23)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--runner-binary", type=Path)
    args = parser.parse_args()
    if args.warm_samples < 1:
        parser.error("--warm-samples must be positive")
    env = dict(os.environ)
    if args.runner_binary:
        args.scratch.mkdir(parents=True, exist_ok=True)
        binary = args.runner_binary.resolve()
    else:
        args.scratch.mkdir(parents=True, exist_ok=False)
        runner_source = ROOT / "crates/kmp-mcp/src/bin/kmp-lease-cardinality-runner.rs"
        if runner_source.exists():
            raise SystemExit(f"refusing to overwrite {runner_source}")
        runner_source.parent.mkdir(parents=True, exist_ok=True)
        runner_source.write_text(RUNNER, encoding="utf-8")
        try:
            subprocess.run(
                ["cargo", "build", "--locked", "-p", "kmp-mcp", "--bin", "kmp-lease-cardinality-runner"],
                cwd=ROOT,
                env=env,
                check=True,
            )
        finally:
            runner_source.unlink(missing_ok=True)
        binary = ROOT / "target/debug/kmp-lease-cardinality-runner"
    if args.build_only:
        print(binary)
        return 0
    rows = []
    raw_samples = []
    for stale in (0, 1_000, 10_000):
        case = args.scratch / f"case-{stale}"
        case.mkdir()
        cold = subprocess.run(
            [str(binary), str(case / "cold"), str(stale), "0"],
            cwd=ROOT,
            env=env,
            check=True,
            capture_output=True,
            text=True,
        )
        warm = subprocess.run(
            [str(binary), str(case / "warm"), str(stale), str(args.warm_samples)],
            cwd=ROOT,
            env=env,
            check=True,
            capture_output=True,
            text=True,
        )
        cold_row = json.loads(cold.stdout)
        warm_row = json.loads(warm.stdout)
        raw_samples.append({"cold_process": cold_row, "warm_process": warm_row})
        samples = warm_row["warm_acquire_ns"]
        rows.append({
            "stale_files": stale,
            "file_count_after": warm_row["file_count_after"],
            "directory_metadata_bytes": warm_row["directory_metadata_bytes"],
            "cold_acquire_ms": cold_row["cold_acquire_ns"] / 1_000_000,
            "warm_samples": len(samples),
            "warm_p50_ms": percentile(samples, 0.50),
            "warm_p95_ms": percentile(samples, 0.95),
            "warm_min_ms": min(samples) / 1_000_000,
            "warm_max_ms": max(samples) / 1_000_000,
        })
    cpu_model, hardware_sha256 = hardware_fingerprint()
    result = {
        "issue": 774,
        "runner": str(Path(__file__).relative_to(ROOT)),
        "runner_sha256": digest(Path(__file__)),
        "runner_binary_sha256": digest(binary),
        "git_commit": command_output("git", "rev-parse", "HEAD"),
        "rustc": command_output("rustc", "-Vv"),
        "cargo_profile": "dev, workspace .cargo/config.toml",
        "host": {
            "system": platform.platform(),
            "machine": platform.machine(),
            "cpu_model": cpu_model,
            "hardware_sha256": hardware_sha256,
        },
        "source_hashes": {
            "crates/kmp-mcp/src/lifecycle/adapters/store_lease_files.rs": digest(
                ROOT / "crates/kmp-mcp/src/lifecycle/adapters/store_lease_files.rs"
            ),
            "crates/kmp-mcp/src/lifecycle/adapters/store_session_lease.rs": digest(
                ROOT / "crates/kmp-mcp/src/lifecycle/adapters/store_session_lease.rs"
            ),
            "crates/kmp-mcp/src/lifecycle/adapters/store_removal_guard.rs": digest(
                ROOT / "crates/kmp-mcp/src/lifecycle/adapters/store_removal_guard.rs"
            ),
        },
        "method": "direct StoreSessionLease::acquire for one canonical store; unrelated files are empty and never opened",
        "warm_samples_requested": args.warm_samples,
        "percentile_method": "nearest rank: ceil(n * fraction) - 1, zero-based",
        "raw_samples": raw_samples,
        "results": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
