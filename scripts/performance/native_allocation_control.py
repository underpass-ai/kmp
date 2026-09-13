#!/usr/bin/env python3
"""Count native allocation requests separately from uninstrumented latency.

Arguments: BASELINE CANDIDATE COUNTER_SO ARTIFACT_ROOT DATA_ROOT OUT SCRATCH.
Run after both paired runners, before removing their scratch seed stores.
Linux/glibc only; compile native_allocation_counter.c first. Each process runs
initialize and four identical queries, including its first catalogue build.
All four complete responses must match the uninstrumented reference response.
"""
import json
import os
from pathlib import Path
import shutil
import sys
from temporal_body_selection import Client, dump, sha


def main():
    baseline, candidate, counter, artifacts, data, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    dump(out / "environment.json", {
        "baseline_sha256": sha(baseline), "candidate_sha256": sha(candidate),
        "counter_sha256": sha(counter), "counter_source_sha256": sha(Path(__file__).with_name("native_allocation_counter.c")),
        "runner_sha256": sha(Path(__file__)), "scope": "process startup, initialize and four identical queries",
        "platform": "Linux/glibc LD_PRELOAD allocation entry points; all process threads",
        "metric": "allocation request count and cumulative requested bytes, not live heap",
        "latency": "do not interpret instrumented times", "concurrency": 1,
    })
    rows = []
    specs = [("temporal-final", "entries-32-body-1024", "kmp_goto"),
             ("temporal-final", "entries-1024-body-32768", "kmp_goto")]
    specs += [("recall-final", shape, tool) for shape in ["entries-32-body-1024", "entries-256-body-4096"] for tool in ["kmp_wake", "kmp_ask"]]
    for run, fixture, tool in specs:
        with (artifacts / run / f"{fixture}-before.jsonl").open() as stream:
            reference = next(row for row in map(json.loads, stream)
                             if row["request"].get("params", {}).get("name") == tool)
        args = reference["request"]["params"]["arguments"]
        expected = reference["response"]["result"]
        name = f"{run}-{fixture}-{tool}"
        row = {"fixture": fixture, "tool": tool, "queries": 4}
        for side, binary in [("before", baseline), ("after", candidate)]:
            store = scratch / name / side
            shutil.copytree(data / run / fixture / "seed", store)
            trace_path = out / f"{name}-{side}.jsonl"
            previous = os.environ.get("LD_PRELOAD")
            os.environ["LD_PRELOAD"] = str(counter)
            try:
                with trace_path.open("w") as trace:
                    client = Client(binary, store, trace)
                    try:
                        for _ in range(4):
                            response, _ = client.call(tool, args)
                            assert response == expected, (name, side, "instrumented response changed")
                    finally:
                        client.close()
            finally:
                if previous is None:
                    del os.environ["LD_PRELOAD"]
                else:
                    os.environ["LD_PRELOAD"] = previous
            stderr = (store / "stderr.log").read_text()
            (out / f"{name}-{side}-stderr.log").write_text(stderr)
            counts = [json.loads(line) for line in stderr.splitlines() if line.startswith('{"native_allocation_control":true,')]
            assert len(counts) == 1, (name, side, "missing native counter")
            row[side] = counts[0]
        rows.append(row)
        dump(out / "results.json", rows)
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
