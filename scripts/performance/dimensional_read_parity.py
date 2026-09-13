#!/usr/bin/env python3
"""Compare complete native MCP results for #775 on copies of one synthetic store.

Usage: dimensional_read_parity.py BASELINE_MCP CANDIDATE_MCP OUTPUT SCRATCH
This is correctness replay, not a latency measurement. No model calls.
"""
import copy
import json
import shutil
import sys
from pathlib import Path

from temporal_body_selection import Client, dump, fixture, sha


def main():
    before, after, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    abouts = [f"project:dimensional-{i}" for i in range(4)]
    try:
        with (out / "seed.jsonl").open("w") as trace:
            client = Client(before, scratch / "seed", trace)
            try:
                for about in abouts:
                    seed, _ = fixture(8, 256)
                    seed = json.loads(json.dumps(seed).replace("project:temporal-perf", about))
                    seed["idempotency_key"] = f"fixture-v1:{about}"
                    client.call("kmp_ingest", seed)
            finally:
                client.close()
        choices = [
            {"scope": "all_abouts", "mode": "only", "include": ["task"]},
            {"scope": "all_abouts", "selectors": [{"key": "task", "op": "in", "values": ["proof"]}]},
            {"scope": "all_abouts", "selectors": [{"key": "task", "op": "in", "values": ["proof"]},
                {"key": "topic", "op": "notexists"}]},
            {"scope": "abouts", "abouts": [abouts[3], abouts[0]], "selectors": [{"key": "topic", "op": "in", "values": ["shared"]}]},
        ]
        queries = []
        for choice in choices:
            base = {"about": abouts[2], "dimensions": choice, "budget": {"max_bytes": 500000, "detail": "full"}}
            queries.append(("kmp_wake", copy.deepcopy(base)))
            queries.append(("kmp_ask", {**copy.deepcopy(base), "question": "Which quantity excludes B?"}))
            for axis in ["occurred", "observed", "ingested", "validity", "default"]:
                query = {**copy.deepcopy(base), "at": {"time": "2026-09-10T10:00:04Z"},
                    "axis": axis, "limit": {"entries": 3}, "include": {"evidence": True, "relations": True}}
                if axis == "default":
                    query.pop("axis")
                queries.append(("kmp_goto", query))
        responses = {}
        for side, binary in [("before", before), ("after", after)]:
            shutil.copytree(scratch / "seed", scratch / side)
            with (out / f"{side}.jsonl").open("w") as trace:
                client = Client(binary, scratch / side, trace)
                try:
                    responses[side] = [client.call(tool, args)[0] for tool, args in queries]
                finally:
                    client.close()
        for i, (left, right) in enumerate(zip(responses["before"], responses["after"], strict=True)):
            assert left == right, (i, queries[i], "complete result differs")
        dump(out / "summary.json", {"complete_equal_results": len(queries), "tool_calls": len(queries) * 2,
            "before_sha256": sha(before), "after_sha256": sha(after), "runner_sha256": sha(Path(__file__)),
            "scope": "4 abouts, positive and negative whole-entry selectors, explicit scope and five clocks",
            "timings": "not interpreted; correctness replay may overlap validation workloads"})
        print(json.dumps({"complete_equal_results": len(queries)}))
    finally:
        shutil.rmtree(scratch)


if __name__ == "__main__":
    main()
