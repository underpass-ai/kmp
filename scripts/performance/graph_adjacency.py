#!/usr/bin/env python3
"""Run the #767 scoped-neighborhood control in ABBA process order.

Usage: graph_adjacency.py BASELINE_EXAMPLE CANDIDATE_EXAMPLE OUTPUT [SAMPLES]
Both binaries must use the same Cargo profile and benchmark source. Linux
/usr/bin/time reports whole-process CPU and peak RSS, including fixture writes.
"""

import hashlib
import json
import math
import platform
import statistics
import subprocess
import sys
from pathlib import Path


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def percentile(values, proportion):
    return sorted(values)[math.ceil(len(values) * proportion) - 1]


def main():
    before, after, output = [Path(value).resolve() for value in sys.argv[1:4]]
    samples = int(sys.argv[4]) if len(sys.argv) > 4 else 30
    output.mkdir(parents=True, exist_ok=False)
    example = Path("crates/kmp-adapter-embedded/examples/graph_adjacency_benchmark.rs")
    write(output / "environment.json", {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu": Path("/proc/cpuinfo").read_text(),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "baseline_sha256": digest(before),
        "candidate_sha256": digest(after),
        "runner_sha256": digest(Path(__file__)),
        "example_sha256": digest(example),
        "profile": "dev; parent workspace Cargo configuration; same settings for both binaries",
        "order": ["before", "after", "after", "before"],
        "process_concurrency": 1,
        "samples_per_process": samples,
        "cold_definition": "first read after seeding in a fresh process; OS cache is not evicted",
        "resources_scope": "whole process including fixture writes and all neighborhood reads",
        "allocations": "not measured; peak RSS and operation-retained payload bounds reported",
        "physical_io": "not measured",
        "model_calls": 0,
    })
    summary = []
    raw = output / "runs.jsonl"
    for shape in ["small", "medium", "high_degree", "large_payload"]:
        groups = {"before": [], "after": []}
        expected_digest = None
        for run, side in enumerate(["before", "after", "after", "before"]):
            stem = output / f"{shape}-{run}-{side}"
            with stem.with_suffix(".json").open("w") as stdout:
                subprocess.run(
                    ["/usr/bin/time", "-v", "-o", str(stem.with_suffix(".resources")),
                     str(before if side == "before" else after), shape, str(samples)],
                    stdout=stdout, check=True, timeout=240,
                )
            process = json.loads(stem.with_suffix(".json").read_text())
            resources = stem.with_suffix(".resources").read_text()
            attributes = {key.strip(): value.strip() for line in resources.splitlines()
                          if ": " in line for key, value in [line.split(": ", 1)]}
            process["process_cpu_seconds"] = (
                float(attributes["User time (seconds)"]) + float(attributes["System time (seconds)"])
            )
            process["process_peak_rss_kib"] = int(attributes["Maximum resident set size (kbytes)"])
            if expected_digest is None:
                expected_digest = process["result_digest"]
            assert process["result_digest"] == expected_digest, (shape, side, "result mismatch")
            groups[side].append(process)
            with raw.open("a") as stream:
                stream.write(json.dumps({"shape": shape, "run": run, "side": side,
                                         "process": process, "resources": resources}) + "\n")
        row = {"shape": shape, "result_digest": expected_digest}
        for side, processes in groups.items():
            warm = [sample["elapsed_us"] for process in processes for sample in process["reads"][1:]]
            row[side] = {
                "first_read_us": [process["reads"][0]["elapsed_us"] for process in processes],
                "warm_p50_us": statistics.median(warm),
                "warm_p95_us": percentile(warm, 0.95),
                "process_cpu_seconds": [process["process_cpu_seconds"] for process in processes],
                "process_peak_rss_kib": [process["process_peak_rss_kib"] for process in processes],
                "neighbors": sorted({process["neighbors"] for process in processes}),
                "relations": sorted({process["relations"] for process in processes}),
            }
        summary.append(row)
        write(output / "summary.json", summary)
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
