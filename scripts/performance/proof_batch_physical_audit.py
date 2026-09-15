#!/usr/bin/env python3
"""Summarize opt-in SQLite/phase profiles captured by proof acceptance #539."""
from __future__ import annotations

import hashlib
import gzip
import json
import math
from pathlib import Path
import statistics
import sys


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def percentiles(values: list[int]) -> dict:
    ordered = sorted(values)
    return {
        "p50": statistics.median(ordered),
        "p95": ordered[math.ceil(len(ordered) * .95) - 1],
        "total": sum(ordered),
    }


def operation_profiles(capture: Path, rows: list[dict], side: str) -> list[list[dict]]:
    ids = [row["id"] for row in rows]
    assert len(ids) == len(set(ids)), f"duplicate profile ids in {capture.name}"
    by_id = {row["id"]: row for row in rows}
    operations = []
    current = None
    request_ids = []
    with gzip.open(capture, "rt") as source:
        for line in source:
            exchange = json.loads(line)
            request = exchange["request"]
            request_ids.append(request["id"])
            assert request["id"] in by_id, f"missing profile for request {request['id']}"
            profile = by_id[request["id"]]
            expected_tool = request.get("params", {}).get("name")
            assert profile["method"] == request["method"]
            assert profile["tool"] == expected_tool
            assert profile["nesting_errors"] == 0
            assert profile["sql"]["statements"] == profile["sql"]["profiles"]
            assert profile["sql"]["vm_step_invalid_deltas"] == 0
            if request.get("method") != "tools/call":
                continue
            params = request["params"]
            tool = params["name"]
            if tool not in {"kmp_trace", "kmp_inspect"}:
                continue
            phase_paths = {"/".join(phase["path"]) for phase in profile["phases"]}
            assert "rpc.dispatch" in phase_paths
            assert f"rpc.dispatch/backend.{tool.removeprefix('kmp_')}" in phase_paths
            assert "rpc.dispatch/dispatch.validate_schema" in phase_paths
            assert "rpc.dispatch/encoding.jsonrpc_string" in phase_paths
            if tool == "kmp_trace" and params["arguments"].get("page", {}).get("cursor") is None:
                if current:
                    operations.append(current)
                current = [profile]
            else:
                assert current is not None, "operation continuation without initial Trace"
                current.append(profile)
            if side == "batch" and tool == "kmp_trace":
                response = exchange["response"]["result"]["structuredContent"]
                if response["page"]["has_more"] is False:
                    operations.append(current)
                    current = None
    if current:
        operations.append(current)
    assert request_ids == ids, "request/profile order or bijection changed"
    return operations


def summarize(rows: list[dict], operations: list[list[dict]]) -> dict:
    tools = {}
    phase_totals = {}
    for row in rows:
        tool = row.get("tool") or row.get("method")
        if tool not in {"kmp_trace", "kmp_inspect"}:
            continue
        tools.setdefault(tool, []).append(row)
        for phase in row["phases"]:
            path = "/".join(phase["path"])
            current = phase_totals.setdefault(path, {"calls": 0, "inclusive_ns": 0})
            current["calls"] += phase["calls"]
            current["inclusive_ns"] += phase["inclusive_ns"]
    return {
        "tools": {
            tool: {
                "rpc_profiles": len(selected),
                "sql_statements": percentiles([row["sql"]["statements"] for row in selected]),
                "sql_rows": percentiles([row["sql"]["rows"] for row in selected]),
                "sqlite_vm_steps": percentiles([row["sql"]["vm_steps"] for row in selected]),
                "sqlite_profile_ns": percentiles([row["sql"]["profile_ns"] for row in selected]),
                "logical_read_calls": sum(
                    item["calls"] for row in selected for item in row["logical_reads"]
                ),
                "body_batches": sum(row["bodies"]["batches"] for row in selected),
                "body_single_reads": sum(row["bodies"]["single_reads"] for row in selected),
                "body_slots": sum(row["bodies"]["slots"] for row in selected),
                "nesting_errors": sum(row["nesting_errors"] for row in selected),
            }
            for tool, selected in sorted(tools.items())
        },
        "phase_totals": dict(sorted(phase_totals.items())),
        "complete_operations": {
            "count": len(operations),
            "rpc_counts": sorted({len(operation) for operation in operations}),
            **{
                metric: percentiles([
                    sum(row["sql"][metric] for row in operation) for operation in operations
                ])
                for metric in ["statements", "rows", "vm_steps", "profile_ns"]
            },
        },
    }


def main() -> None:
    if len(sys.argv) not in (2, 3):
        raise SystemExit("usage: proof_batch_physical_audit.py ARTIFACT_DIR [OUTPUT]")
    root = Path(sys.argv[1]).resolve()
    captures = {}
    for path in sorted(root.glob("*-physical.json.gz")):
        shape = path.name.removesuffix("-physical.json.gz")
        with gzip.open(path, "rt") as source:
            sides = json.load(source)
        summarized_sides = {
            side: summarize(
                rows,
                operation_profiles(root / f"{shape}-{side}.jsonl.gz", rows, side),
            )
            for side, rows in sorted(sides.items())
        }
        if shape.startswith("064-"):
            assert min(summarized_sides["batch"]["complete_operations"]["rpc_counts"]) > 1
        captures[shape] = {
            "source": path.name,
            "sha256": digest(path),
            "sides": summarized_sides,
        }
    acceptance = json.loads((root / "summary.json").read_text())
    assert {row["shape"] for row in acceptance} == set(captures)
    for row in acceptance:
        assert row["equivalence"]["selected_entries"] in {1, 8, 64}
        assert row["warm"]["oracle"]["samples"] == row["warm"]["batch"]["samples"]
        assert row["warm"]["oracle"]["samples"] > 0
    result = {
        "contract": "kmp.proof-batch-physical-audit.v1",
        "scope": "opt-in per-RPC inclusive phases and actual embedded SQLite trace_v2 events",
        "limitations": [
            "inclusive phase totals overlap and must not be summed as exclusive wall time",
            "SQLite profile time is observer-reported statement execution time, not physical disk I/O",
            "VmHWM remains process-wide in summary.json and is not attributed to one query",
        ],
        "auditor": Path(__file__).name,
        "auditor_sha256": digest(Path(__file__).resolve()),
        "captures": captures,
    }
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if len(sys.argv) == 3:
        Path(sys.argv[2]).write_text(encoded)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
