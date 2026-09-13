#!/usr/bin/env python3
"""Reproducible synthetic native and real-browser baseline, issue #765.

Dependencies: Python 3.11+, tiktoken==0.14.0; browser mode also requires
playwright==1.58.0 and its Chromium. See --help and the development report.
Never point scratch at a real store. Outputs and scratch must not exist.
"""
import argparse
import gzip
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

import tiktoken

from baseline_client import BaselineClient
from baseline_fixtures import SHAPES, make_fixture, read_cases
from baseline_journey import journey
from temporal_body_selection import dump, sha, stats


def measure(client, cases, samples, warmups):
    rows, fingerprints = [], {}
    for name, tool, arguments in cases:
        expected = None
        warm = []
        for i in range(samples + warmups + 1):
            result, row = client.call(tool, arguments)
            if expected is None:
                expected = result
                fingerprints[name] = result
            assert result == expected, f"non-deterministic {name} read at sample {i}"
            if name in ("wake", "ask"):
                assert result["structuredContent"]["proof"]["evidence"], "positive fixture returned no evidence"
            if i == 0:
                rows.append({"operation": name, "cache": "first-operation-in-process", **stats([row]),
                    "compact_response_tokens": row["compact_response_tokens"]})
            elif i > warmups:
                warm.append(row)
        rows.append({"operation": name, "cache": "warm", **stats(warm),
            "compact_response_tokens": sorted({r["compact_response_tokens"] for r in warm}),
            "proc_io_totals": {key: sum(r["proc_io_delta"][key] for r in warm)
                               for key in warm[0]["proc_io_delta"]},
            "mcp_calls": len(warm)})
    return rows, fingerprints


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("scratch", type=Path)
    parser.add_argument("--commit", required=True, help="full source commit of the measured executable")
    parser.add_argument("--profile", required=True, help="exact build profile/configuration")
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--shapes", nargs="+", choices=SHAPES, default=list(SHAPES))
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--browser-samples", type=int, default=5)
    parser.add_argument("--allocation-counter", type=Path,
                        help="optional native_allocation_counter.c shared object; separate run only")
    args = parser.parse_args()
    if args.samples < 1 or args.warmups < 0 or args.browser_samples < 1:
        parser.error("samples must be positive and warmups non-negative")
    binary, out, scratch = (p.resolve() for p in (args.binary, args.output, args.scratch))
    counter = args.allocation_counter.resolve() if args.allocation_counter else None
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    encoder = tiktoken.get_encoding("cl100k_base")
    environment = {
        "commit": args.commit, "binary_sha256": sha(binary), "profile": args.profile,
        "platform": platform.platform(), "architecture": platform.machine(),
        "cpu_count": os.cpu_count(),
        "memory": Path("/proc/meminfo").read_text(),
        "python": platform.python_version(), "token_encoder": "cl100k_base",
        "tiktoken": importlib.metadata.version("tiktoken"),
        "token_scope": "compact complete JSON-RPC response counted client-side after timing; not billed model tokens",
        "samples": args.samples, "warmups": args.warmups,
        "concurrency": "one native server; browser requests concurrent only during browser journeys",
        "cache": "fresh server process over persisted seed; OS cache not flushed; first operation and warm separated",
        "startup": "Popen through initialize, including first response token accounting; excluded from read latency",
        "cpu": "server /proc stat clock ticks; browser CDP cumulative process metrics separately",
        "cpu_tick_hz": os.sysconf("SC_CLK_TCK"), "peak_memory": "native VmHWM process lifetime, not per-call live heap",
        "io": "Linux per-process rchar/wchar/syscr/syscw and read_bytes/write_bytes; not SQLite page/cache counters or device-level physical I/O",
        "query_counts": "exact MCP calls and HTTP requests; SQL statements, cache hits and page reads not instrumented",
        "allocations": "separate instrumented process request count/cumulative bytes; not live heap" if counter else "unavailable in this uninstrumented run; use --allocation-counter separately",
        "counter_sha256": sha(counter) if counter else None, "model_calls": 0,
        "runner_hashes": {p.name: sha(p) for p in Path(__file__).parent.glob("baseline*.py")},
        "fixture_helper_sha256": sha(Path(__file__).with_name("temporal_body_selection.py")),
    }
    # Keep hardware description separate from the CPU measurement semantics.
    environment["lscpu"] = subprocess.check_output(["lscpu", "--json"], text=True)
    dump(out / "environment.json", environment)
    results = []
    try:
        for shape in args.shapes:
            folder = out / shape
            folder.mkdir()
            seed, refs = make_fixture(shape)
            dump(folder / "fixture.json", seed)
            dump(folder / "shape.json", {"entries": len(refs),
                "source_bytes_per_entry": SHAPES[shape][1],
                "relations": len(seed["memory"]["relations"]), "max_degree": SHAPES[shape][2]})
            store = scratch / shape
            with (folder / "seed.jsonl").open("w") as trace:
                client = BaselineClient(binary, store, trace, encoder)
                try:
                    client.call("kmp_ingest", seed)
                finally:
                    client.close()
            # Reopen the persisted fixture: seed/write work is outside reads.
            with (folder / "native.jsonl").open("w") as trace:
                client = BaselineClient(binary, store, trace, encoder, counter)
                try:
                    rows, fingerprints = measure(client, read_cases(seed["about"], refs), args.samples, args.warmups)
                    dump(folder / "read-results.json", fingerprints)
                    rows.append({"operation": "process-startup", "elapsed_ms": client.startup_ms})
                    if args.browser:
                        from playwright.sync_api import sync_playwright
                        with sync_playwright() as playwright:
                            with playwright.chromium.launch(headless=True, args=["--enable-unsafe-swiftshader"]) as browser:
                                environment["browser"] = {"version": browser.version,
                                    "playwright": importlib.metadata.version("playwright"),
                                    "gpu": "headless Chromium, unsafe SwiftShader enabled; not physical GPU performance"}
                                dump(folder / "journeys.json", journey(browser, client, seed["about"], refs,
                                     args.browser_samples, folder))
                    writes = []
                    for i in range(args.samples):
                        written, row = client.call("kmp_write_memory", {"about": seed["about"],
                            "actor": "synthetic-baseline", "idempotency_key": f"baseline-write-{i}",
                            "labels": {"task": ["synthetic-baseline"]},
                            "observed_at": "2026-09-12T10:00:00Z",
                            "memories": [{"id": "sample", "kind": "observation",
                                "summary": f"Synthetic write sample {i} records quantity 17.",
                                "evidence": f"Non-personal fixture source {i}: quantity 17."}]})
                        assert written["structuredContent"].get("accepted"), written
                        writes.append(row)
                    rows.append({"operation": "write-memory", "cache": "sequential-new-writes", **stats(writes)})
                    results.append({"shape": shape, "rows": rows})
                finally:
                    client.close()
            stderr = (store / "stderr.log").read_text()
            (folder / "stderr.jsonl").write_text(stderr)
            phases, allocations = [], []
            for line in stderr.splitlines():
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if event.get("target") == "kmp_mcp::query_phases":
                    phases.append(event)
                if event.get("native_allocation_control"):
                    allocations.append(event)
            dump(folder / "phases.json", phases)
            dump(folder / "allocations.json", allocations)
            if counter:
                assert len(allocations) == 1, "allocation counter failed to report"
            dump(out / "results.json", results)
            print(json.dumps({"completed": shape}), flush=True)
        dump(out / "environment.json", environment)
        # Keep complete raw traces, compressed without changing their content.
        manifest = []
        for path in sorted(out.rglob("*")):
            if path.is_file():
                original_sha, original_size = sha(path), path.stat().st_size
                if path.suffix == ".jsonl" or path.name == "read-results.json":
                    compressed = path.with_suffix(path.suffix + ".gz")
                    with path.open("rb") as source, gzip.GzipFile(filename=str(compressed), mode="wb", mtime=0) as target:
                        shutil.copyfileobj(source, target)
                    path.unlink()
                    path = compressed
                manifest.append({"file": str(path.relative_to(out)), "sha256": sha(path),
                    "uncompressed_sha256": original_sha, "uncompressed_bytes": original_size})
        dump(out / "manifest.json", manifest)
    except BaseException as error:
        dump(out / "failure.json", {"type": type(error).__name__, "message": str(error),
            "interpretation": "incomplete diagnostic run; excluded from performance comparison"})
        for path in scratch.rglob("stderr.log"):
            destination = out / "diagnostics" / path.relative_to(scratch)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
        raise
    finally:
        # This directory was created exclusively by this invocation.
        shutil.rmtree(scratch)


if __name__ == "__main__":
    main()
