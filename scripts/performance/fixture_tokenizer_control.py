"""Run the #766 recall control against the MCP fixture backend.

The shared runner defaults to the embedded backend. This wrapper overrides
only the child process backend during construction so the call reaches
`fixture_backend::read_recall_fixture_tool_result`, the production caller
whose estimator ownership changed in #766.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "performance"))
import temporal_body_selection as transport  # noqa: E402
from temporal_body_selection import dump, fixture as base_fixture, sha, stats  # noqa: E402


def fixture(count, body):
    seed, refs = base_fixture(count, body)
    for entry in seed["memory"]["entries"]:
        for coordinate in entry["coordinates"]:
            coordinate.pop("valid_until", None)
    return seed, refs


def fixture_client(binary, store, trace):
    original_popen = transport.subprocess.Popen

    def spawn(args, **kwargs):
        env = dict(kwargs["env"])
        env["KMP_MCP_BACKEND"] = "fixture"
        kwargs["env"] = env
        return original_popen(args, **kwargs)

    transport.subprocess.Popen = spawn
    try:
        return transport.Client(binary, store, trace)
    finally:
        transport.subprocess.Popen = original_popen


def main():
    baseline, candidate, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    dump(out / "environment.json", {
        "baseline_sha256": sha(baseline), "candidate_sha256": sha(candidate),
        "runner_sha256": sha(Path(__file__)),
        "transport_runner_sha256": sha(Path(__file__).with_name("temporal_body_selection.py")),
        "profile": "dev, workspace Cargo config", "trials": ["trial-1", "trial-2", "trial-3"],
        "fixture_backend_ignores_seeded_corpus": True,
        "warm_samples": 20, "warmups": 3, "startup_included_in_query_latency": False,
        "concurrency": 1, "backend": "fixture", "model_calls": 0,
    })
    rows = []
    for trial, (count, body) in enumerate([(32, 1024), (128, 2048), (256, 4096)], start=1):
        name = f"trial-{trial}"
        seed, _ = fixture(count, body)
        dump(out / f"{name}-input.json", seed)
        expected_by_tool = {}
        for side, binary in [("before", baseline), ("after", candidate)]:
            for tool in ["kmp_wake", "kmp_ask"]:
                store = scratch / name / f"{side}-{tool}"
                trace = (out / f"{name}-{side}-{tool}.jsonl").open("w")
                client = fixture_client(binary, store, trace)
                try:
                    args = {"about": "project:kmp", "budget": {"max_bytes": 2_000_000, "detail": "full"}}
                    if tool == "kmp_ask":
                        args["question"] = "quantity 17 excludes B"
                    result, first = client.call(tool, args)
                    if tool in expected_by_tool:
                        assert result == expected_by_tool[tool], (name, side, tool, "cross-binary mismatch")
                    else:
                        expected_by_tool[tool] = result
                    expected = result
                    samples = []
                    for i in range(23):
                        result, measured = client.call(tool, args)
                        assert result == expected, (name, side, tool, i)
                        if i >= 3:
                            samples.append(measured)
                    row = next((item for item in rows if item["fixture"] == name and item["tool"] == tool), None)
                    if row is None:
                        row = {"fixture": name, "tool": tool, "equality_cases": 1, "first_query_ms": {}, "before": {}, "after": {}}
                        rows.append(row)
                    row["first_query_ms"][side] = first["elapsed_ms"]
                    row[side] = stats(samples)
                    dump(out / "results.json", rows)
                finally:
                    client.close()
                    trace.close()
    print(json.dumps(rows))


if __name__ == "__main__":
    main()
