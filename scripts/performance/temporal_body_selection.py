#!/usr/bin/env python3
"""Paired local MCP control for #770. Synthetic stores; no model calls.

Usage: python3 scripts/performance/temporal_body_selection.py BASELINE CANDIDATE OUT SCRATCH
Build both binaries with the same Cargo profile. OUT holds evidence; SCRATCH
holds disposable stores. Each case is seeded once and copied after closing it.
"""
import copy
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


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Client:
    def __init__(self, binary, store, trace):
        store.mkdir(parents=True, exist_ok=True)
        self.trace = trace
        self.serial = 0
        env = {k: v for k, v in os.environ.items() if not k.startswith(("KMP_", "KERNEL_"))}
        env.update(KMP_MCP_BACKEND="embedded", KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR="off", XDG_DATA_HOME=str(store / "xdg"),
                   XDG_STATE_HOME=str(store / "state"))
        self.stderr = (store / "stderr.log").open("w")
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.stderr, text=True, bufsize=1,
            env=env, cwd=store)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "temporal-body-control", "version": "1"}})

    def resources(self):
        stat = Path(f"/proc/{self.process.pid}/stat").read_text().split()
        status = dict(line.split(":", 1) for line in Path(f"/proc/{self.process.pid}/status").read_text().splitlines())
        return {"cpu_ticks": int(stat[13]) + int(stat[14]),
                "peak_rss_kib": int(status["VmHWM"].split()[0])}

    def rpc(self, method, params):
        self.serial += 1
        req = {"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params}
        resources = self.resources()
        start = time.perf_counter_ns()
        self.process.stdin.write(json.dumps(req, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        if not self.selector.select(60):
            raise TimeoutError("MCP call exceeded 60 seconds")
        line = self.process.stdout.readline()
        elapsed = (time.perf_counter_ns() - start) / 1e6
        response = json.loads(line)
        after = self.resources()
        row = {"request": req, "response": response, "elapsed_ms": elapsed,
            "response_bytes": len(line.encode()), "cpu_ticks": after["cpu_ticks"] - resources["cpu_ticks"],
            "peak_rss_kib": after["peak_rss_kib"]}
        self.trace.write(json.dumps(row, ensure_ascii=False) + "\n")
        self.trace.flush()
        assert response.get("id") == self.serial, response
        assert "error" not in response, response
        result = response["result"]
        assert not result.get("isError"), result
        return result, row

    def call(self, tool, arguments):
        return self.rpc("tools/call", {"name": tool, "arguments": arguments})

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
        self.selector.close()
        self.process.stdout.close()
        self.stderr.close()


def fixture(count, source_bytes):
    about = "project:temporal-perf"
    refs = [f"{about}:entry:observation:fact-{i:04}" for i in range(count)]
    entries, evidence = [], []
    for i, ref in enumerate(refs):
        at = f"2026-09-10T10:{i // 60:02}:{i % 60:02}Z"
        coordinate = {"dimension": "task", "scope_id": "proof", "sequence": i + 1,
            "observed_at": at, "ingested_at": at, "valid_from": at,
            "valid_until": "2026-09-11T00:00:00Z"}
        if i % 5:
            coordinate["occurred_at"] = at
        coordinates = [coordinate]
        if i % 3 == 0:
            extra = {**coordinate, "dimension": "topic", "scope_id": "shared"}
            coordinates.append(extra)
        entries.append({"id": ref, "kind": "observation", "text": f"Fact {i}: quantity 17 excludes B. " * 8,
                        "coordinates": coordinates})
        text = (f"Source {i}: quantity 17 excludes B. " * (source_bytes // 32 + 1))[:source_bytes - 1] + "X"
        evidence.append({"id": f"evidence:{about}:source-{i:04}", "supports": [ref],
                         "text": text, "source": f"fixture:S{i}", "time": at,
                         "support_clocks": {"observed_at": at, "ingested_at": at}})
    # Shared, old and late dependencies, with an equal timestamp tie.
    entries[2]["coordinates"][0]["observed_at"] = entries[1]["coordinates"][0]["observed_at"]
    relations = [{"from": refs[i], "to": refs[0], "rel": "uses_background", "class": "evidential", "confidence": "high",
        "why": "This observation uses the original count as its reference.",
        "evidence": "The source identifies observation zero as the reference count.",
        "clocks": {"observed_at": "2026-09-10T10:00:01Z"}} for i in range(1, min(count, 12))]
    return {"about": about, "idempotency_key": "fixture-v1", "memory": {
        "dimensions": [{"id": "proof", "kind": "task"}, {"id": "shared", "kind": "topic"}],
        "entries": entries, "evidence": evidence, "relations": relations}}, refs


def queries(refs):
    base = {"about": "project:temporal-perf", "axis": "observed",
            "limit": {"entries": 3}, "budget": {"max_bytes": 2_000_000},
            "include": {"evidence": True, "relations": True, "raw_refs": True}}
    cases = []
    for axis in ["default", "occurred", "observed", "ingested", "validity"]:
        for tool, key in [("kmp_goto", "at"), ("kmp_forward", "from"), ("kmp_rewind", "from"), ("kmp_near", "around")]:
            q = {**copy.deepcopy(base), "axis": axis, key: {"time": "2026-09-10T10:00:06Z"}}
            if axis == "default":
                q.pop("axis")
            if tool == "kmp_near":
                q.pop("limit")
                q["window"] = {"before_entries": 2, "after_entries": 2}
            cases.append((tool, q))
    for deps in [False, True]:
        q = {**copy.deepcopy(base), "at": {"time": "2026-09-10T10:00:06Z"}}
        q["include"]["dependencies"] = deps
        cases.append(("kmp_goto", q))
    for includes in [{"evidence": False, "relations": True}, {"evidence": False, "raw_refs": True}]:
        cases.append(("kmp_forward", {**copy.deepcopy(base), "from": {"ref": refs[1]}, "include": includes}))
    cases.append(("kmp_forward", {**copy.deepcopy(base), "interval": {
        "start": "2026-09-10T10:00:01Z", "end": "2026-09-10T10:00:06Z"}}))
    q = {**copy.deepcopy(base), "at": {"time": "2026-09-10T10:00:06Z"},
         "dimensions": {"selectors": [{"key": "topic", "op": "in", "values": ["shared"]}]}}
    cases.append(("kmp_goto", q))
    return cases


def stats(rows):
    times = [row["elapsed_ms"] for row in rows]
    return {"samples": len(times), "p50_ms": statistics.median(times),
        "p95_ms": sorted(times)[math.ceil(len(times) * .95) - 1],
        "cpu_ms": sum(row["cpu_ticks"] for row in rows) * 1000 / os.sysconf("SC_CLK_TCK"),
        "peak_rss_kib": max(row["peak_rss_kib"] for row in rows),
        "response_bytes": sorted(set(row["response_bytes"] for row in rows))}


def main():
    baseline, candidate, out, scratch = map(lambda p: Path(p).resolve(), sys.argv[1:])
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    dump(out / "environment.json", {"baseline_sha256": sha(baseline), "candidate_sha256": sha(candidate),
        "runner_sha256": sha(Path(__file__)), "platform": platform.platform(), "cpu_count": os.cpu_count(),
        "profile": "dev, workspace Cargo config", "pairs": 20, "warmups": 3,
        "cache": "process-first read and warm; OS cache not flushed", "concurrency": 1,
        "startup_included_in_query_latency": False, "token_encoder": "not measured",
        "allocations": "not measured; process peak RSS reported", "physical_io": "not measured",
        "cpu_clock_tick_hz": os.sysconf("SC_CLK_TCK"), "model_calls": 0})
    rows = []
    for count, body in [(32, 1024), (256, 16384), (1024, 32768)]:
        name = f"entries-{count}-body-{body}"
        folder = scratch / name
        seed, refs = fixture(count, body)
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
        clients = {"before": Client(baseline, folder / "before", traces["before"]),
                   "after": Client(candidate, folder / "after", traces["after"])}
        try:
            cases = queries(refs)
            timed = cases[8]  # observed Goto, three selected entries
            measurements = {side: [] for side in clients}
            first = {}
            expected = None
            for side, client in clients.items():
                result, first[side] = client.call(*timed)
                packet = result["structuredContent"]
                assert len(packet["entries"]) == 3, "timed fixture must return three entries"
                sources = [item for item in packet["proof"]["evidence"] if item["id"].startswith("detail:evidence:")]
                assert len(sources) == 3, "timed fixture must return all three canonical sources"
                assert all(len(item["text"].encode()) == body for item in sources)
                if expected is None:
                    expected = result
                assert result == expected, (name, side, "first-read mismatch")
            for tool, args in cases:
                left, _ = clients["before"].call(tool, args)
                right, _ = clients["after"].call(tool, args)
                if args.get("include", {}).get("dependencies"):
                    assert any(len(group["member_refs"]) > 1 for group in left["structuredContent"]["proof"]["groups"]), "dependency control must reach an antecedent"
                if left != right:
                    dump(out / "mismatch.json", {"case": name, "tool": tool, "args": args, "before": left, "after": right})
                    raise AssertionError("Complete response mismatch; see mismatch.json")
            # Response pagination must preserve each complete proof item.
            tool, args = cases[21]
            args = copy.deepcopy(args)
            args["page"] = {"entries": 2}
            pages = 0
            while True:
                left, _ = clients["before"].call(tool, args)
                right, _ = clients["after"].call(tool, args)
                assert left == right, (name, "paged mismatch")
                pages += 1
                assert pages < 500
                content = left["structuredContent"]
                if not content["page"]["has_more"]:
                    break
                args = content["next_actions"][0]["arguments"]
            for i in range(23):
                order = ["before", "after"] if i % 2 == 0 else ["after", "before"]
                for side in order:
                    result, measurement = clients[side].call(*timed)
                    assert result == expected, (name, side, i)
                    if i >= 3:
                        measurements[side].append(measurement)
            row = {"fixture": name, "equality_cases": len(cases), "proof_pages": pages,
                   "first_query_ms": {side: first[side]["elapsed_ms"] for side in clients},
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
