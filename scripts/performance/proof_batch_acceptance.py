#!/usr/bin/env python3
"""Compare joint Trace proof with the quiescent Trace + Inspect oracle for #539.

Usage: proof_batch_acceptance.py BINARY OUT SCRATCH [SAMPLES]

BINARY must be a frozen copy outside Cargo target. OUT must not exist. The
runner seeds one store, closes it, copies that state for both paths, proves
literal equivalence before timing, and removes SCRATCH on normal or failed exit.
Query timings exclude process startup; process-first reads are reported
separately and are not OS-cold-cache measurements. No model is called.
"""
from __future__ import annotations

import copy
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

from temporal_body_selection import Client, dump, sha, stats


ABOUT = "project:proof-batch-acceptance"
TOTAL_RESPONSE_BUDGET = 8_000_000


def content(result):
    return result["structuredContent"]


def fixture(entries: int, source_mode: str) -> tuple[dict, list[str]]:
    refs = [f"{ABOUT}:observation:{index:03}" for index in range(entries)]
    records = []
    for index, reference in enumerate(refs):
        second = index % 60
        minute = index // 60
        at = f"2026-09-01T10:{minute:02}:{second:02}Z"
        records.append({
            "id": reference,
            "kind": "observation",
            "text": f"Entry {index:03}: literal π body for {source_mode}.",
            "summary_en": f"Entry {index:03} literal body.",
            "metadata": {"fixture": source_mode, "ordinal": str(index)},
            "coordinates": [{
                "dimension": "task", "scope_id": "proof-batch",
                "occurred_at": at, "observed_at": at, "ingested_at": at,
                "valid_from": at, "valid_until": "2026-10-01T00:00:00Z",
                "sequence": index + 1,
            }],
        })
    relations = []
    for index in range(entries - 1):
        relations.append({
            "from": refs[index], "to": refs[index + 1], "rel": "depends_on",
            "class": "causal", "confidence": "high",
            "why": f"Chain step {index:03} precedes {index + 1:03}.",
            "evidence": f"Signed chain register {index:03}->{index + 1:03}.",
            "clocks": {
                "observed_at": "2026-09-02T00:00:00Z",
                "ingested_at": "2026-09-02T00:00:01Z",
            },
        })
    if source_mode == "shared":
        groups = [refs[index:index + 8] for index in range(0, entries, 8)]
    else:
        groups = [[reference] for reference in refs]
    evidence = []
    for index, supported in enumerate(groups):
        evidence.append({
            "id": f"evidence:{ABOUT}:source:{index:03}",
            "supports": supported,
            "text": f"Source {index:03}: exact λ evidence for {','.join(supported)}. " + "x" * 384,
            "source": f"fixture-register:{source_mode}:{index:03}",
            "time": "2026-09-01T09:00:00Z",
            "support_clocks": {
                "observed_at": "2026-09-01T09:00:01Z",
                "ingested_at": "2026-09-01T09:00:02Z",
            },
            "metadata": {"fixture": source_mode, "ordinal": str(index)},
        })
    return ({
        "about": ABOUT,
        "idempotency_key": f"proof-batch:{entries}:{source_mode}",
        "memory": {
            "dimensions": [{"id": "proof-batch", "kind": "task"}],
            "entries": records,
            "relations": relations,
            "evidence": evidence,
        },
    }, refs)


def trace_arguments(refs: list[str], proof: bool) -> dict:
    return {
        "about": ABOUT,
        "from": refs[0],
        "to": refs[-1],
        "axis": "observed",
        "as_of": {"time": "2026-09-15T00:00:00Z"},
        "search": {
            "proof": proof,
            "direction": "outgoing",
            "relations": ["depends_on"],
            "max_nodes": 512,
            "max_edges": 4096,
            "max_depth": 256,
            "max_states": 4096,
        },
        "budget": {"max_bytes": TOTAL_RESPONSE_BUDGET},
    }


def route_refs(trace: dict) -> list[str]:
    selected = {trace["search"]["from"]}
    for route in trace["routes"]:
        for index in route["edge_indexes"]:
            edge = trace["trace"][index]
            selected.update([edge["from"], edge["to"]])
    return sorted(selected)


def aggregate(rows: list[dict]) -> dict:
    return {
        "elapsed_ms": sum(row["elapsed_ms"] for row in rows),
        "response_bytes": sum(row["response_bytes"] for row in rows),
        "cpu_ticks": sum(row["cpu_ticks"] for row in rows),
        "peak_rss_kib": max(row["peak_rss_kib"] for row in rows),
        "rpc_calls": len(rows),
    }


def oracle(client: Client, refs: list[str]) -> tuple[dict, dict]:
    plain_result, first = client.call("kmp_trace", trace_arguments(refs, False))
    plain = content(plain_result)
    assert plain["page"]["has_more"] is False
    selected = route_refs(plain)
    inspections = []
    rows = [first]
    for reference in selected:
        result, row = client.call("kmp_inspect", {
            "about": ABOUT,
            "ref": reference,
            "include": {"details": True, "incoming": True, "outgoing": True, "raw": True},
            "budget": {"max_bytes": TOTAL_RESPONSE_BUDGET},
        })
        value = content(result)
        assert value["page"]["has_more"] is False
        inspections.append(value)
        rows.append(row)
    return ({"trace": plain, "inspections": inspections}, aggregate(rows))


def batched(client: Client, refs: list[str]) -> tuple[dict, dict]:
    arguments = trace_arguments(refs, True)
    arguments["page"] = {"entries": 32}
    first = None
    stable_warnings = None
    rows = []
    sections = {name: [] for name in ["trace", "objects", "supports", "gaps"]}
    expected_offset = 0
    total = None
    for _ in range(40):
        result, row = client.call("kmp_trace", arguments)
        value = content(result)
        rows.append(row)
        if first is None:
            first = copy.deepcopy(value)
            for name in sections:
                first[name] = []
            stable_warnings = [
                warning for warning in value["warnings"]
                if warning != "response is partial; execute next_actions to continue the same selection"
            ]
        else:
            for name in ["summary", "search", "routes", "proof", "quality"]:
                assert value[name] == first[name], f"{name} changed between proof pages"
            assert [
                warning for warning in value["warnings"]
                if warning != "response is partial; execute next_actions to continue the same selection"
            ] == stable_warnings
        partial_warning = "response is partial; execute next_actions to continue the same selection" in value["warnings"]
        assert partial_warning == value["page"]["has_more"]
        page = value["page"]
        returned = sum(len(value[name]) for name in sections)
        assert page["returned"] == returned, "page.returned does not match its typed sections"
        assert page["offset"] == expected_offset, "proof page offset is not contiguous"
        total = page["total"] if total is None else total
        assert page["total"] == total, "proof page total changed between continuations"
        assert page["offset"] + returned <= total, "proof page exceeds its declared total"
        for name in sections:
            sections[name].extend(value[name])
        expected_offset += returned
        if page["has_more"] is False:
            assert page["next_cursor"] in (None, "")
            assert page["offset"] + returned == total, "final proof page does not reach total"
            assert not any(action["tool"] == "kmp_trace" for action in value["next_actions"])
            break
        assert returned > 0, "proof pagination stalled despite the sufficient fixture budget"
        cursor = page["next_cursor"]
        assert cursor, "partial proof page omitted its cursor"
        actions = [
            action for action in value["next_actions"]
            if action["tool"] == "kmp_trace"
            and action["arguments"].get("page", {}).get("cursor") == cursor
        ]
        assert len(actions) == 1, "one exact continuation must advance the interrupted proof"
        arguments = actions[0]["arguments"]
    else:
        raise AssertionError("proof pagination did not terminate")
    assert expected_offset == total, "reconstructed proof does not contain page.total items"
    for name, items in sections.items():
        first[name] = items
    first["page"] = {
        "returned": sum(len(items) for items in sections.values()),
        "total": total,
        "has_more": False,
        "next_cursor": None,
        "offset": 0,
    }
    first["next_actions"] = []
    first["warnings"] = stable_warnings
    return first, aggregate(rows)


def operation_stats(rows: list[dict]) -> dict:
    result = stats(rows)
    result["rpc_calls"] = sorted({row["rpc_calls"] for row in rows})
    total_client = [row["total_client_ms"] for row in rows]
    total_client.sort()
    result["total_client_p50_ms"] = total_client[len(total_client) // 2]
    result["total_client_p95_ms"] = total_client[math.ceil(len(total_client) * .95) - 1]
    return result


def optional(target: dict, key: str, value):
    if value not in (None, "", {}, []):
        target[key] = value


def compare(oracle_value: dict, batch: dict) -> dict:
    plain = oracle_value["trace"]
    inspections = oracle_value["inspections"]
    assert batch["trace"] == plain["trace"], "typed path relations or their order changed"
    assert batch["routes"] == plain["routes"], "route indexes or order changed"
    selected = route_refs(plain)
    assert [item["object"]["ref"] for item in inspections] == selected

    by_ref = {item["ref"]: item for item in batch["objects"]}
    assert len(by_ref) == len(batch["objects"]), "proof objects contain duplicate refs"
    source_evidence = {}
    oracle_supports = []
    for inspected in inspections:
        reference = inspected["object"]["ref"]
        actual = by_ref[reference]
        expected_object = inspected["object"]
        for field in ["ref", "kind", "text", "metadata"]:
            assert actual[field] == expected_object[field], (reference, field)
        assert actual.get("source", "") == expected_object.get("source", "")
        raw = next(item for item in inspected["raw"] if item["ref"] == reference)
        assert actual["coordinates"] == raw["coordinates"]
        assert actual["content_hash"] == raw["content_hash"]
        assert actual["revision"] == raw["revision"]
        for evidence in inspected["evidence"]:
            previous = source_evidence.setdefault(evidence["id"], evidence)
            assert previous == evidence, f"shared source {evidence['id']} changed between inspections"
        oracle_supports.extend(
            relation for relation in inspected["links"]["incoming"]
            if relation["rel"] == "supports"
        )

    oracle_supports.sort(key=lambda row: (row["to"], row["from"], row["rel"]))
    assert batch["supports"] == oracle_supports, "source arrows, explanations, clocks or order changed"
    expected_source_order = sorted(source_evidence)
    actual_source_order = [item["ref"] for item in batch["objects"] if item["ref"] not in selected]
    assert actual_source_order == expected_source_order, "source order changed"
    assert set(by_ref) == set(selected) | set(expected_source_order), "proof object set changed"
    for reference in expected_source_order:
        source = by_ref[reference]
        evidence = copy.deepcopy(source_evidence[reference])
        # Inspect presents stored evidence through MemoryEvidence and adds this
        # projection-only classifier. Trace presents the same record as its
        # typed source object; kind=memory_evidence carries that role there.
        # Require the only shape difference explicitly before comparing every
        # persisted byte and field.
        assert evidence.get("metadata", {}).pop("proof_role", None) == "stored_evidence"
        if not evidence.get("metadata"):
            evidence.pop("metadata", None)
        expected = {
            "id": source["ref"],
            "supports": [row["to"] for row in batch["supports"] if row["from"] == reference],
            "text": source["text"],
        }
        optional(expected, "source", source.get("source"))
        optional(expected, "time", source.get("time"))
        optional(expected, "support_clocks", source.get("support_clocks"))
        optional(expected, "metadata", source.get("metadata"))
        assert expected == evidence, f"source body/provenance mismatch for {reference}"

    assert batch["proof"]["missing_refs_count"] == 0
    assert batch["proof"]["missing_bodies_count"] == 0
    assert batch["proof"]["incomplete_entries_count"] == 0
    assert batch["proof"]["clock_unknown_entries_count"] == 0
    assert batch["proof"]["stop_reason"] == "sources_enumerated"
    assert batch["proof"]["complete_groups"] == [0]
    assert batch["proof"]["incomplete_groups"] == []
    assert batch["gaps"] == [], "complete fixture returned proof gaps"
    assert batch["proof"]["body_bytes"] == sum(
        len(item["text"].encode()) for item in batch["objects"]
    )
    return {
        "selected_entries": len(selected),
        "selected_sources": len(source_evidence),
        "objects": len(batch["objects"]),
        "supports": len(batch["supports"]),
        "body_bytes": batch["proof"]["body_bytes"],
        "qualified_projection_difference": {
            "inspect.evidence[].metadata.proof_role": "stored_evidence",
            "trace.objects[].kind": "memory_evidence",
        },
    }


def stable_digest(value) -> str:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def profile_rows(stderr_path: Path) -> list[dict]:
    prefix = "EVAL539_PROFILE "
    return [
        json.loads(line[len(prefix):])
        for line in stderr_path.read_text().splitlines()
        if line.startswith(prefix)
    ]


def assert_total_budget(operation: dict, side: str) -> None:
    used = operation["response_bytes"]
    assert used <= TOTAL_RESPONSE_BUDGET, (
        f"{side} traversal used {used} response bytes, exceeding the shared "
        f"{TOTAL_RESPONSE_BUDGET}-byte total budget"
    )


def main() -> None:
    if len(sys.argv) not in (4, 5):
        raise SystemExit(__doc__)
    binary, out, scratch = [Path(value).resolve() for value in sys.argv[1:4]]
    samples = int(sys.argv[4]) if len(sys.argv) == 5 else 20
    if samples < 2:
        raise SystemExit("SAMPLES must be at least 2")
    if not binary.is_file():
        raise SystemExit(f"binary does not exist: {binary}")
    if "target" in binary.parts:
        raise SystemExit("freeze a copy of the binary outside Cargo target before measuring")
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    environment = {
        "binary_sha256": sha(binary),
        "runner_sha256": sha(Path(__file__)),
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "source_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True).strip()),
        "profile": "dev; workspace Cargo configuration",
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu_count": os.cpu_count(),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "samples_per_path": samples,
        "warmups": 3,
        "concurrency": 1,
        "startup_included_in_query_latency": False,
        "process_first_definition": "first read after initialize on a copied, quiescent store; OS cache not evicted",
        "timed_scope": "sum of Client wait elapsed for each RPC in an operation; excludes JSON parsing, gzip trace writes and client work between RPCs",
        "total_client_scope": "wall time around the complete logical operation, including client JSON parsing and work between RPCs; excludes process startup",
        "shared_total_response_budget_bytes": TOTAL_RESPONSE_BUDGET,
        "per_response_budget_note": "each MCP request still declares the API's response ceiling; acceptance separately rejects either complete traversal when cumulative response bytes exceed the shared total",
        "oracle_scope": "one base Trace plus Inspect(details,incoming,outgoing,raw) for every selected entry; a rich audit path, not the minimum possible client inspection",
        "allocations": "not measured; process VmHWM reported",
        "physical_io": "not measured",
        "model_calls": 0,
    }
    dump(out / "environment.json", environment)
    summaries = []
    try:
        for entries, source_mode in [(1, "distinct"), (8, "distinct"), (64, "distinct"), (64, "shared")]:
            preparation_started = time.perf_counter_ns()
            name = f"{entries:03}-{source_mode}"
            shape = scratch / name
            seed_store = shape / "seed"
            packet, refs = fixture(entries, source_mode)
            dump(out / f"{name}-fixture.json", packet)
            with gzip.open(out / f"{name}-seed.jsonl.gz", "wt") as trace:
                client = Client(binary, seed_store, trace, {"EVAL539_PROFILE": "1"})
                try:
                    client.call("kmp_ingest", packet)
                finally:
                    client.close()
            shutil.copytree(seed_store, shape / "oracle")
            shutil.copytree(seed_store, shape / "batch")
            preparation_ms = (time.perf_counter_ns() - preparation_started) / 1e6
            traces = {
                side: gzip.open(out / f"{name}-{side}.jsonl.gz", "wt")
                for side in ["oracle", "batch"]
            }
            clients = {}
            startup_ms = {}
            for side in ["oracle", "batch"]:
                started = time.perf_counter_ns()
                clients[side] = Client(
                    binary, shape / side, traces[side], {"EVAL539_PROFILE": "1"}
                )
                startup_ms[side] = (time.perf_counter_ns() - started) / 1e6
            try:
                started = time.perf_counter_ns()
                oracle_first, oracle_first_row = oracle(clients["oracle"], refs)
                oracle_first_row["total_client_ms"] = (time.perf_counter_ns() - started) / 1e6
                started = time.perf_counter_ns()
                batch_first, batch_first_row = batched(clients["batch"], refs)
                batch_first_row["total_client_ms"] = (time.perf_counter_ns() - started) / 1e6
                assert_total_budget(oracle_first_row, "oracle")
                assert_total_budget(batch_first_row, "batch")
                equivalence = compare(oracle_first, batch_first)
                expected = {
                    "oracle": stable_digest(oracle_first),
                    "batch": stable_digest(batch_first),
                }
                for _ in range(3):
                    compare(oracle(clients["oracle"], refs)[0], batched(clients["batch"], refs)[0])
                measured = {"oracle": [], "batch": []}
                for index in range(samples):
                    order = ["oracle", "batch"] if index % 2 == 0 else ["batch", "oracle"]
                    values = {}
                    for side in order:
                        started = time.perf_counter_ns()
                        values[side], row = (
                            oracle(clients[side], refs) if side == "oracle"
                            else batched(clients[side], refs)
                        )
                        row["total_client_ms"] = (time.perf_counter_ns() - started) / 1e6
                        assert_total_budget(row, side)
                        assert stable_digest(values[side]) == expected[side]
                        measured[side].append(row)
                    compare(values["oracle"], values["batch"])
                summary = {
                    "shape": name,
                    "preparation_ms": preparation_ms,
                    "startup_initialize_ms": startup_ms,
                    "shared_total_response_budget_bytes": TOTAL_RESPONSE_BUDGET,
                    "equivalence": equivalence,
                    "logical_rpc_calls": {
                        "oracle": oracle_first_row["rpc_calls"],
                        "batch": batch_first_row["rpc_calls"],
                    },
                    "process_first": {"oracle": oracle_first_row, "batch": batch_first_row},
                    "warm": {side: operation_stats(rows) for side, rows in measured.items()},
                    "response_digests": expected,
                }
                summaries.append(summary)
                dump(out / "summary.json", summaries)
                print(json.dumps(summary, ensure_ascii=False), flush=True)
            finally:
                for client in clients.values():
                    client.close()
                for trace in traces.values():
                    trace.close()
                dump(out / f"{name}-physical.json", {
                    side: profile_rows(shape / side / "stderr.log")
                    for side in ["oracle", "batch"]
                })
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
