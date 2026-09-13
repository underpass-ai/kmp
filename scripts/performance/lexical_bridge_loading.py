#!/usr/bin/env python3
"""Paired #773 lexical-bridge startup control; no model calls.

Usage: lexical_bridge_loading.py BASELINE CANDIDATE BRIDGE COUNTER OUT SCRATCH
Both binaries must use the same Cargo profile. The runner alternates them,
measures initialization with the exact same bridge, compares complete MCP
responses on bilingual queries, and runs the native allocation counter once.
"""

import hashlib
import json
import math
import os
from pathlib import Path
import platform
import selectors
import shutil
import statistics
import subprocess
import sys
import time


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def condition_bridge(path, warm):
    with path.open("rb") as stream:
        if warm:
            while stream.read(1024 * 1024):
                pass
            return "read entire bridge immediately before process start"
        if hasattr(os, "posix_fadvise"):
            os.posix_fadvise(stream.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
            return "POSIX_FADV_DONTNEED requested immediately before process start"
    return "cold-cache request unavailable on this platform"


class Client:
    def __init__(self, binary, store, bridge, counter=None):
        store.mkdir(parents=True, exist_ok=True)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("KMP_", "KERNEL_"))}
        env.update(KMP_MCP_BACKEND="embedded", KMP_MCP_DATA_DIR=str(store),
                   KMP_LEXICAL_BRIDGE=str(bridge), KMP_VIEWER_ADDR="off",
                   XDG_DATA_HOME=str(store / "xdg"),
                   XDG_STATE_HOME=str(store / "state"))
        if counter is not None:
            env["LD_PRELOAD"] = str(counter)
        self.stderr_path = store / "stderr.log"
        self.stderr = self.stderr_path.open("w")
        self.started_ns = time.perf_counter_ns()
        self.process = subprocess.Popen(
            [str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr, text=True, bufsize=1, env=env, cwd=store)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.serial = 0

    def rpc(self, method, params):
        self.serial += 1
        request = {"jsonrpc": "2.0", "id": self.serial,
                   "method": method, "params": params}
        started = time.perf_counter_ns()
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        if not self.selector.select(60):
            raise TimeoutError("MCP call exceeded 60 seconds")
        response = json.loads(self.process.stdout.readline())
        elapsed_ms = (time.perf_counter_ns() - started) / 1e6
        assert response.get("id") == self.serial, response
        assert "error" not in response, response
        assert not response["result"].get("isError"), response
        return response["result"], elapsed_ms

    def initialize(self):
        result, rpc_ms = self.rpc("initialize", {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "lexical-bridge-loading", "version": "1"}})
        status = dict(line.split(":", 1) for line in
                      Path(f"/proc/{self.process.pid}/status").read_text().splitlines())
        return result, {
            "spawn_to_initialize_ms": (time.perf_counter_ns() - self.started_ns) / 1e6,
            "initialize_rpc_ms": rpc_ms,
            "rss_kib": int(status["VmRSS"].split()[0]),
            "peak_rss_kib": int(status["VmHWM"].split()[0]),
        }

    def call(self, tool, arguments):
        return self.rpc("tools/call", {"name": tool, "arguments": arguments})[0]

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=10)
        self.selector.close()
        self.process.stdout.close()
        self.stderr.close()


def stats(rows):
    values = [row["spawn_to_initialize_ms"] for row in rows]
    return {
        "samples": len(values),
        "first_ms": values[0],
        "p50_ms": statistics.median(values),
        "p95_ms": sorted(values)[math.ceil(len(values) * 0.95) - 1],
        "rss_kib_p50": statistics.median(row["rss_kib"] for row in rows),
        "peak_rss_kib_max": max(row["peak_rss_kib"] for row in rows),
    }


def measure(binary, side, bridge, condition, iteration, scratch):
    cache_control = condition_bridge(bridge, condition == "warm")
    client = Client(binary, scratch / "timing" / condition / side / str(iteration), bridge)
    try:
        result, row = client.initialize()
        row.update(side=side, condition=condition, iteration=iteration,
                   cache_control=cache_control)
        return result, row
    finally:
        client.close()


def allocation(binary, side, bridge, counter, scratch, out):
    client = Client(binary, scratch / "allocations" / side, bridge, counter)
    try:
        client.initialize()
    finally:
        client.close()
    stderr = client.stderr_path.read_text()
    (out / f"allocations-{side}-stderr.log").write_text(stderr)
    rows = [json.loads(line) for line in stderr.splitlines()
            if line.startswith('{"native_allocation_control":true,')]
    assert len(rows) == 1, (side, "native allocation report missing")
    return rows[0]


def parity(baseline, candidate, bridge, scratch, out):
    seed = {
        "about": "project:lexical-bridge-performance",
        "idempotency_key": "performance-773-parity-v1",
        "memory": {
            "dimensions": [{"id": "parity", "kind": "task"}],
            "entries": [
                {"id": "project:lexical-bridge-performance:entry:observation:spanish",
                 "kind": "observation",
                 "text": "valvula fabrica noche cliente reunion",
                 "coordinates": [{"dimension": "task", "scope_id": "parity", "sequence": 1}]},
                {"id": "project:lexical-bridge-performance:entry:observation:control",
                 "kind": "observation",
                 "text": "canteen backlog deployment",
                 "coordinates": [{"dimension": "task", "scope_id": "parity", "sequence": 2}]},
            ],
            "evidence": [], "relations": [],
        },
    }
    seed_client = Client(baseline, scratch / "parity" / "seed", bridge)
    try:
        seed_client.initialize()
        seed_client.call("kmp_ingest", seed)
    finally:
        seed_client.close()

    questions = ["valve factory night", "customer meeting", "night valve customer"]
    responses = {}
    for side, binary in [("before", baseline), ("after", candidate)]:
        store = scratch / "parity" / side
        shutil.copytree(scratch / "parity" / "seed", store)
        client = Client(binary, store, bridge)
        try:
            client.initialize()
            responses[side] = [client.call("kmp_ask", {
                "about": seed["about"], "question": question,
                "budget": {"detail": "full", "max_bytes": 100_000}})
                for question in questions]
        finally:
            client.close()
    dump(out / "parity-before.json", responses["before"])
    dump(out / "parity-after.json", responses["after"])
    assert responses["before"] == responses["after"], "lexical responses changed"
    return {"questions": questions, "responses_identical": True,
            "response_sha256": sha(out / "parity-before.json")}


def main():
    baseline, candidate, bridge, counter, out, scratch = [
        Path(value).resolve() for value in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    try:
        rows = []
        initialized = None
        for condition in ["cold", "warm"]:
            for iteration in range(23):
                order = [("before", baseline), ("after", candidate)]
                if iteration % 2:
                    order.reverse()
                for side, binary in order:
                    result, row = measure(binary, side, bridge, condition,
                                          iteration, scratch)
                    if initialized is None:
                        initialized = result
                    assert result == initialized, (side, "initialize response changed")
                    if iteration >= 3:
                        rows.append(row)
        dump(out / "timing-samples.json", rows)
        summary = {condition: {side: stats([
            row for row in rows if row["condition"] == condition and row["side"] == side])
            for side in ["before", "after"]} for condition in ["cold", "warm"]}
        allocations = {side: allocation(binary, side, bridge, counter, scratch, out)
                       for side, binary in [("before", baseline), ("after", candidate)]}
        parity_result = parity(baseline, candidate, bridge, scratch, out)
        dump(out / "results.json", {"timing": summary, "allocations": allocations,
                                     "parity": parity_result})
        dump(out / "environment.json", {
            "baseline_sha256": sha(baseline), "candidate_sha256": sha(candidate),
            "bridge_sha256": sha(bridge), "bridge_bytes": bridge.stat().st_size,
            "counter_sha256": sha(counter), "runner_sha256": sha(Path(__file__)),
            "profile": "Cargo dev profile under workspace .cargo/config.toml",
            "platform": platform.platform(), "cpu_count": os.cpu_count(),
            "warmups_per_condition": 3, "samples_per_condition": 20,
            "timing_scope": "process spawn through MCP initialize response",
            "allocation_scope": "process startup and one initialize; all glibc allocation entry points",
            "allocation_metric": "request count and cumulative requested bytes, not live heap",
            "model_calls": 0,
        })
        print(json.dumps({"timing": summary, "allocations": allocations,
                          "parity": parity_result}, ensure_ascii=False))
    finally:
        shutil.rmtree(scratch)


if __name__ == "__main__":
    main()
