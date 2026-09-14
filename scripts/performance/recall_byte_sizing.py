#!/usr/bin/env python3
"""Compare #769 projection binaries and test-only serialization-pass controls.

Usage: recall_byte_sizing.py BASELINE CANDIDATE BASELINE_TEST CANDIDATE_TEST
       COUNTER_SO OUT [SAMPLES]

Latency runs never preload the allocation counter. Allocation controls compare
otherwise identical one-warmup processes with zero or N additional projections;
their difference is cumulative requests/bytes, not live or retained heap.
"""

import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys


SHAPES = ["empty", "small", "medium", "shortened_core", "oversized_item"]
PASS_PREFIX = "PERFORMANCE_769 "
ALLOC_PREFIX = '{"native_allocation_control":true,'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(binary, *args, preload=None):
    env = os.environ.copy()
    if preload is not None:
        env["LD_PRELOAD"] = str(preload)
    completed = subprocess.run(
        [str(binary), *map(str, args)],
        check=True,
        text=True,
        capture_output=True,
        env=env,
    )
    return completed.stdout, completed.stderr


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(len(ordered) * fraction))]


def summarize(values):
    return {
        "count": len(values),
        "min": min(values),
        "p50": statistics.median(values),
        "p95": percentile(values, 0.95),
        "max": max(values),
    }


def pass_counts(test_binary):
    stdout, _ = invoke(
        test_binary,
        "v1beta1::recall_projection::core_fit_tests::recall_sizing_serialization_pass_control",
        "--exact",
        "--ignored",
        "--nocapture",
    )
    rows = []
    for line in stdout.splitlines():
        if PASS_PREFIX in line:
            rows.append(json.loads(line.split(PASS_PREFIX, 1)[1]))
    assert len(rows) == 3, rows
    return rows


def allocation(binary, counter, shape, iterations):
    stdout, stderr = invoke(binary, "allocation", shape, iterations, preload=counter)
    payload = json.loads(stdout)
    counts = [
        json.loads(line)
        for line in stderr.splitlines()
        if line.startswith(ALLOC_PREFIX)
    ]
    assert len(counts) == 1, stderr
    return {"output": payload, "counts": counts[0]}


def main():
    if len(sys.argv) not in (7, 8):
        raise SystemExit(__doc__)
    before, after, before_test, after_test, counter, out = map(
        lambda raw: Path(raw).resolve(), sys.argv[1:7]
    )
    samples = int(sys.argv[7]) if len(sys.argv) == 8 else 30
    out.mkdir(parents=True, exist_ok=False)
    raw_rows = []

    parity = {}
    for shape in SHAPES:
        sides = {}
        for side, binary in [("before", before), ("after", after)]:
            stdout, _ = invoke(binary, "parity", shape)
            sides[side] = json.loads(stdout)
        assert sides["before"] == sides["after"], shape
        parity[shape] = sides["before"]

    timing = {}
    for shape in SHAPES:
        by_side = {"before": [], "after": []}
        for side, binary in [
            ("before", before),
            ("after", after),
            ("after", after),
            ("before", before),
        ]:
            stdout, stderr = invoke(binary, "timing", shape, samples)
            assert not stderr, stderr
            row = json.loads(stdout)
            assert row["digest"] == parity[shape]["page_hashes"][0]
            by_side[side].extend(row["samples_us"])
            raw_rows.append({"kind": "timing", "side": side, **row})
        timing[shape] = {side: summarize(values) for side, values in by_side.items()}

    allocations = {}
    allocation_iterations = 25
    for shape in ["medium", "shortened_core", "oversized_item"]:
        allocations[shape] = {}
        for side, binary in [("before", before), ("after", after)]:
            zero = allocation(binary, counter, shape, 0)
            many = allocation(binary, counter, shape, allocation_iterations)
            assert zero["output"]["digest"] == many["output"]["digest"]
            allocations[shape][side] = {
                "iterations": allocation_iterations,
                "additional_requests": many["counts"]["requests"]
                - zero["counts"]["requests"],
                "additional_requested_bytes": many["counts"]["requested_bytes"]
                - zero["counts"]["requested_bytes"],
                "zero": zero["counts"],
                "many": many["counts"],
            }

    passes = {"before": pass_counts(before_test), "after": pass_counts(after_test)}
    results = {
        "parity": parity,
        "timing_us": timing,
        "serialization_passes": passes,
        "allocations": allocations,
    }
    (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    with gzip.GzipFile(
        filename=str(out / "timing-raw.jsonl.gz"), mode="wb", mtime=0
    ) as handle:
        for row in raw_rows:
            handle.write((json.dumps(row, separators=(",", ":")) + "\n").encode())
    environment = {
        "baseline_sha256": sha(before),
        "candidate_sha256": sha(after),
        "baseline_test_sha256": sha(before_test),
        "candidate_test_sha256": sha(after_test),
        "counter_sha256": sha(counter),
        "runner_sha256": sha(Path(__file__)),
        "python": sys.version,
        "platform": platform.platform(),
        "samples_per_process": samples,
        "timing_processes_per_side_shape": 2,
        "timing_instrumentation": "none; LD_PRELOAD is absent",
        "allocation_measurement": "Linux/glibc cumulative allocation requests and requested bytes; zero-vs-N process difference; not live or retained heap",
        "physical_io": "not measured",
        "model_calls": 0,
    }
    (out / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    print(json.dumps(results))


if __name__ == "__main__":
    main()
