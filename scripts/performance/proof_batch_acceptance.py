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

import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from temporal_body_selection import Client, dump, sha, stats


ABOUT = "project:proof-batch-acceptance"


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
        "budget": {"max_bytes": 8_000_000},
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
            "budget": {"max_bytes": 8_000_000},
        })
        value = content(result)
        assert value["page"]["has_more"] is False
        inspections.append(value)
        rows.append(row)
    return ({"trace": plain, "inspections": inspections}, aggregate(rows))


def batched(client: Client, refs: list[str]) -> tuple[dict, dict]:
    result, row = client.call("kmp_trace", trace_arguments(refs, True))
    value = content(result)
    assert value["page"]["has_more"] is False
    return value, aggregate([row])


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
    for reference in expected_source_order:
        source = by_ref[reference]
        evidence = source_evidence[reference]
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
    assert batch["proof"]["body_bytes"] == sum(
        len(item["text"].encode()) for item in batch["objects"]
    )
    return {
        "selected_entries": len(selected),
        "selected_sources": len(source_evidence),
        "objects": len(batch["objects"]),
        "supports": len(batch["supports"]),
        "body_bytes": batch["proof"]["body_bytes"],
        "selection_fingerprint": batch["selection_fingerprint"],
    }


def stable_digest(value) -> str:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


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
        "allocations": "not measured; process VmHWM reported",
        "physical_io": "not measured",
        "model_calls": 0,
    }
    dump(out / "environment.json", environment)
    summaries = []
    try:
        for entries, source_mode in [(1, "distinct"), (8, "distinct"), (64, "distinct"), (64, "shared")]:
            name = f"{entries:03}-{source_mode}"
            shape = scratch / name
            seed_store = shape / "seed"
            packet, refs = fixture(entries, source_mode)
            dump(out / f"{name}-fixture.json", packet)
            with gzip.open(out / f"{name}-seed.jsonl.gz", "wt") as trace:
                client = Client(binary, seed_store, trace)
                try:
                    client.call("kmp_ingest", packet)
                finally:
                    client.close()
            shutil.copytree(seed_store, shape / "oracle")
            shutil.copytree(seed_store, shape / "batch")
            traces = {
                side: gzip.open(out / f"{name}-{side}.jsonl.gz", "wt")
                for side in ["oracle", "batch"]
            }
            clients = {
                side: Client(binary, shape / side, traces[side])
                for side in ["oracle", "batch"]
            }
            try:
                oracle_first, oracle_first_row = oracle(clients["oracle"], refs)
                batch_first, batch_first_row = batched(clients["batch"], refs)
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
                        values[side], row = (
                            oracle(clients[side], refs) if side == "oracle"
                            else batched(clients[side], refs)
                        )
                        assert stable_digest(values[side]) == expected[side]
                        measured[side].append(row)
                    compare(values["oracle"], values["batch"])
                summary = {
                    "shape": name,
                    "equivalence": equivalence,
                    "logical_rpc_calls": {"oracle": 1 + entries, "batch": 1},
                    "process_first": {"oracle": oracle_first_row, "batch": batch_first_row},
                    "warm": {side: stats(rows) for side, rows in measured.items()},
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
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
