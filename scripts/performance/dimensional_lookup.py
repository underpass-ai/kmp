#!/usr/bin/env python3
"""Run the #775 native control in ABBA order, with raw samples and process costs.

Usage: dimensional_lookup.py BASELINE_EXAMPLE CANDIDATE_EXAMPLE OUTPUT [SAMPLES]
Build the same dimensional_lookup_benchmark.rs on both source revisions first.
Linux /usr/bin/time reports process CPU and high-water RSS, including seeding.
No OS page-cache eviction is attempted. Run from the candidate repository.
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
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def percentile(values, p):
    return sorted(values)[math.ceil(len(values) * p) - 1]


def main():
    before, after, out = [Path(p).resolve() for p in sys.argv[1:4]]
    samples = int(sys.argv[4]) if len(sys.argv) > 4 else 30
    out.mkdir(parents=True, exist_ok=False)
    write(out / "environment.json", {
        "platform": platform.platform(), "machine": platform.machine(),
        "cpu": Path("/proc/cpuinfo").read_text(),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "before_sha256": digest(before), "after_sha256": digest(after),
        "runner_sha256": digest(Path(__file__)),
        "example_sha256": digest(Path("crates/kmp-adapter-embedded/examples/dimensional_lookup_benchmark.rs")),
        "order": ["before", "after", "after", "before"], "process_concurrency": 1,
        "profile": "dev; workspace Cargo configuration; same settings for both binaries",
        "cold_definition": "first lookup in a freshly seeded process; OS cache is not evicted",
        "resources_scope": "whole process including fixture writes, lookup and scheduling controls",
        "samples_per_process": samples,
    })
    results = []
    raw_runs = out / "runs.jsonl"
    for shape in ["small", "medium", "high_degree", "large_body"]:
        groups = {"before": [], "after": []}
        for run, side in enumerate(["before", "after", "after", "before"]):
            stem = out / f"{shape}-{run}-{side}"
            with stem.with_suffix(".json").open("w") as output:
                subprocess.run(["/usr/bin/time", "-v", "-o", str(stem.with_suffix(".resources")),
                    str(before if side == "before" else after), shape, str(samples)],
                    stdout=output, check=True, timeout=180)
            native_stdout = stem.with_suffix(".json").read_text()
            resources = stem.with_suffix(".resources").read_text()
            process = json.loads(native_stdout)
            attributes = {key.strip(): value.strip() for line in resources.splitlines()
                if ": " in line for key, value in [line.split(": ", 1)]}
            process["process_cpu_seconds"] = float(attributes["User time (seconds)"]) + float(attributes["System time (seconds)"])
            process["process_peak_rss_kib"] = int(attributes["Maximum resident set size (kbytes)"])
            groups[side].append(process)
            with raw_runs.open("a") as stream:
                stream.write(json.dumps({"shape": shape, "run": run, "side": side,
                    "native_stdout": native_stdout, "resources": resources}) + "\n")
        row = {"shape": shape}
        for side, processes in groups.items():
            values = [r["lookup_us"] for process in processes for r in process["lookups"][1:]]
            row[side] = {"warm_p50_us": statistics.median(values), "warm_p95_us": percentile(values, .95),
                "first_lookup_us": [p["lookups"][0]["lookup_us"] for p in processes],
                "scheduling_p50_us": {width: statistics.median(r["elapsed_us"] for p in processes
                    for r in p["schedules"] if r["width"] == width) for width in [1, 4]}}
            reads = [x for p in processes for x in p["mixed"]["read_us"]]
            writes = [x for p in processes for x in p["mixed"]["write_us"]]
            row[side].update({
                "mixed_read_p50_us": statistics.median(reads), "mixed_read_p95_us": percentile(reads, .95),
                "mixed_write_p50_us": statistics.median(writes), "mixed_write_p95_us": percentile(writes, .95),
                "mixed_read_throughput_per_sec": [48 / (p["mixed"]["elapsed_us"] / 1e6) for p in processes],
                "process_cpu_seconds": [p["process_cpu_seconds"] for p in processes],
                "process_peak_rss_kib": [p["process_peak_rss_kib"] for p in processes],
            })
        results.append(row)
        write(out / "summary.json", results)
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
