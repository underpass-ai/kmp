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
    by_id = {row["id"]: row for row in rows}
    operations = []
    current = None
    with gzip.open(capture, "rt") as source:
        for line in source:
            exchange = json.loads(line)
            request = exchange["request"]
            if request.get("method") != "tools/call":
                continue
            params = request["params"]
            tool = params["name"]
            if tool not in {"kmp_trace", "kmp_inspect"}:
                continue
            profile = by_id[request["id"]]
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
    for path in sorted(root.glob("*-physical.json")):
        shape = path.name.removesuffix("-physical.json")
        sides = json.loads(path.read_text())
        captures[shape] = {
            "source": path.name,
            "sha256": digest(path),
            "sides": {
                side: summarize(
                    rows,
                    operation_profiles(root / f"{shape}-{side}.jsonl.gz", rows, side),
                )
                for side, rows in sorted(sides.items())
            },
        }
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
