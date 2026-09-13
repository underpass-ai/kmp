#!/usr/bin/env python3
"""Native paired Wake/Ask admission control. Arguments: BASELINE CANDIDATE OUT SCRATCH.

Uses the temporal runner's transport, resource measurements and synthetic
fixtures. Raw responses are compared before interpreting latency. No model call.
"""
import copy
import json
import os
from pathlib import Path
import platform
import shutil
import sys
from temporal_body_selection import Client, dump, fixture, sha, stats


def main():
    baseline, candidate, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    dump(out / "environment.json", {
        "baseline_sha256": sha(baseline), "candidate_sha256": sha(candidate),
        "runner_sha256": sha(Path(__file__)), "transport_runner_sha256": sha(Path(__file__).with_name("temporal_body_selection.py")),
        "platform": platform.platform(), "cpu_count": os.cpu_count(),
        "profile": "dev, workspace Cargo config", "pairs": 20, "warmups": 3,
        "startup_included": False, "cache": "process-first and warm; OS cache not flushed",
        "concurrency": 1, "allocations": "not measured", "physical_io": "not measured",
        "peak_memory": "process VmHWM including correctness controls", "token_encoder": "not measured", "model_calls": 0,
    })
    rows = []
    for count, body in [(32, 1024), (128, 2048), (256, 4096)]:
        name = f"entries-{count}-body-{body}"
        folder = scratch / name
        seed, refs = fixture(count, body)
        # One selected entry out of sixteen, with a different lane carrying
        # the selector. A correct read must retain the entire candidate text.
        for i, entry in enumerate(seed["memory"]["entries"]):
            entry["coordinates"] = entry["coordinates"][:1]
            if i % 16 == 0:
                entry["coordinates"].append({**entry["coordinates"][0], "dimension": "topic", "scope_id": "shared"})
        dump(out / f"{name}-input.json", seed)
        with (out / f"{name}-seed.jsonl").open("w") as trace:
            client = Client(baseline, folder / "seed", trace)
            try:
                client.call("kmp_ingest", seed)
            finally:
                client.close()
        for side in ["before", "after"]:
            shutil.copytree(folder / "seed", folder / side)
        traces = {side: (out / f"{name}-{side}.jsonl").open("w") for side in ["before", "after"]}
        clients = {"before": Client(baseline, folder / "before", traces["before"]), "after": Client(candidate, folder / "after", traces["after"])}
        try:
            base = {"about": seed["about"], "dimensions": {"mode": "only", "include": ["task"],
                "selectors": [{"key": "topic", "op": "in", "values": ["shared"]}]},
                "budget": {"max_bytes": 2_000_000, "max_entries": 10000, "detail": "full"}}
            for tool in ["kmp_wake", "kmp_ask"]:
                args = {**base, **({"question": "quantity 17 excludes B"} if tool == "kmp_ask" else {})}
                cases = [args]
                for axis in ["occurred", "observed", "ingested", "validity"]:
                    cases.append({**copy.deepcopy(args), "axis": axis, "as_of": {"time": "2026-09-10T10:00:31Z"}})
                cases.append({**copy.deepcopy(args), "interval": {"start": "2026-09-10T10:00:01Z", "end": "2026-09-10T10:00:31Z"}, "axis": "observed"})
                if tool == "kmp_ask":
                    cases.append({**copy.deepcopy(args), "question": "unrelated zeppelin silver"})
                expected = None
                first = {}
                for i, case in enumerate(cases):
                    left, lrow = clients["before"].call(tool, case)
                    right, rrow = clients["after"].call(tool, case)
                    if left != right:
                        dump(out / "mismatch.json", {"fixture": name, "tool": tool, "arguments": case, "before": left, "after": right})
                        raise AssertionError("complete recall mismatch")
                    if i == 0:
                        expected = left
                        first = {"before": lrow["elapsed_ms"], "after": rrow["elapsed_ms"]}
                        sources = [e for e in left["structuredContent"]["proof"]["evidence"] if e["id"].startswith("detail:evidence:")]
                        assert sources and all(len(e["text"].encode()) == body for e in sources), "canonical sources must remain complete"
                measurements = {side: [] for side in clients}
                for i in range(23):
                    for side in (["before", "after"] if i % 2 == 0 else ["after", "before"]):
                        result, measured = clients[side].call(tool, args)
                        assert result == expected, (name, tool, side, i)
                        if i >= 3:
                            measurements[side].append(measured)
                row = {"fixture": name, "tool": tool, "equality_cases": len(cases), "first_query_ms": first,
                       **{side: stats(samples) for side, samples in measurements.items()}}
                rows.append(row)
                dump(out / "results.json", rows)
                print(json.dumps(row), flush=True)
        finally:
            for client in clients.values():
                client.close()
            for trace in traces.values():
                trace.close()


if __name__ == "__main__":
    main()
