#!/usr/bin/env python3
"""Measure ChronoLoom's real agent-open, reload and focus/trace waterfall.

Usage: chronoloom_startup.py BASELINE CANDIDATE OUTPUT SCRATCH [--samples 8]

Each sample uses a fresh embedded store, kmp-mcp process and browser context.
The warm reload reuses that process and context. Timings are uninstrumented;
this runner does not report allocator counters.
"""

import argparse
import gzip
import hashlib
import json
import math
import os
import platform
import queue
import re
import shutil
import statistics
import subprocess
import threading
import time
from collections import defaultdict
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from playwright.sync_api import sync_playwright


VIEWER_URL = re.compile(r"memory viewer at (http://[^;\s]+)")
ABOUT = "project:chronoloom-startup"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def dump_gzip(path, value):
    with gzip.open(path, "wt", encoding="utf-8", compresslevel=9) as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def fixture():
    refs = [f"{ABOUT}:observation:item-{index:02}" for index in range(32)]
    entries = []
    for index, ref in enumerate(refs):
        at = f"2026-09-01T10:00:{index:02}Z"
        entries.append({
            "id": ref,
            "kind": "observation",
            "text": f"Synthetic startup memory {index}: stable proof path.",
            "coordinates": [{
                "dimension": "task",
                "scope_id": "proof",
                "sequence": index + 1,
                "occurred_at": at,
                "observed_at": at,
                "ingested_at": at,
            }],
        })
    relations = [{
        "from": refs[index],
        "to": refs[0],
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "This synthetic observation uses item zero as its fixed reference.",
        "evidence": "The deterministic fixture declares item zero as the reference.",
        "clocks": {
            "occurred_at": "2026-09-01T10:00:01Z",
            "observed_at": "2026-09-01T10:00:01Z",
            "ingested_at": "2026-09-01T10:00:01Z",
        },
    } for index in range(1, 12)]
    return ({
        "about": ABOUT,
        "idempotency_key": "performance-777-startup-v1",
        "memory": {
            "dimensions": [{"id": "proof", "kind": "task"}],
            "entries": entries,
            "relations": relations,
        },
    }, refs)


class Client:
    def __init__(self, binary, store):
        store.mkdir(parents=True, exist_ok=False)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("KMP_", "KERNEL_")) and key != "LD_PRELOAD"}
        env.update(KMP_MCP_BACKEND="embedded", KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR="127.0.0.1:0", XDG_DATA_HOME=str(store / "xdg"),
                   XDG_STATE_HOME=str(store / "state"))
        self.process = subprocess.Popen(
            [str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1, env=env, cwd=store,
        )
        self.serial = 0
        self.stderr = []
        self.viewer_urls = queue.Queue()
        self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
        self.stderr_thread.start()

    def _read_stderr(self):
        for line in self.process.stderr:
            self.stderr.append(line)
            match = VIEWER_URL.search(line)
            if match:
                self.viewer_urls.put(match.group(1))

    def rpc(self, method, params):
        self.serial += 1
        request = {"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params}
        started = time.perf_counter_ns()
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        elapsed_ms = (time.perf_counter_ns() - started) / 1e6
        if not line:
            raise RuntimeError("kmp-mcp closed before answering: " + "".join(self.stderr))
        response = json.loads(line)
        if response.get("id") != self.serial or "error" in response:
            raise RuntimeError(json.dumps(response))
        result = response["result"]
        if result.get("isError"):
            raise RuntimeError(json.dumps(result))
        return result, {"elapsed_ms": elapsed_ms, "response_bytes": len(line.encode())}

    def initialize(self):
        return self.rpc("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "performance-777", "version": "1"},
        })

    def call(self, tool, arguments):
        return self.rpc("tools/call", {"name": tool, "arguments": arguments})

    def viewer_url(self):
        return self.viewer_urls.get(timeout=30)

    def resources(self):
        status = {}
        for line in Path(f"/proc/{self.process.pid}/status").read_text().splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                status[key] = value.strip()
        stat = Path(f"/proc/{self.process.pid}/stat").read_text().split()
        return {
            "rss_kib": int(status["VmRSS"].split()[0]),
            "peak_rss_kib": int(status["VmHWM"].split()[0]),
            "cpu_ticks": int(stat[13]) + int(stat[14]),
        }

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=15)
        self.stderr_thread.join(timeout=5)
        self.process.stdout.close()
        self.process.stderr.close()


def structured(result):
    return result.get("structuredContent", result)


def public_url(url):
    parts = urlsplit(url)
    query = urlencode([(key, "REDACTED" if key == "k" else value)
                       for key, value in parse_qsl(parts.query)])
    return urlunsplit((parts.scheme, parts.netloc, parts.path, query, ""))


def scene_state(page):
    return page.evaluate("""() => {
      const {model, view, sync} = KMP_APP.state;
      return {
        about: model.about,
        total: model.total,
        projected_refs: model.entries.map(entry => entry.ref).sort(),
        lod: model.currentLod,
        projection_revision: model.projection?.revision || null,
        projection_content_hash: model.projection?.content_hash || null,
        sync_revision: sync.revision,
        clock: view.clock,
        selected: view.selectedRef,
        trace_refs: view.trace ? [...view.trace.refs].sort() : [],
        trace_edges: view.trace ? (view.trace.edges || []).map(edge =>
          [edge.source || edge.from, edge.target || edge.to, edge.rel]).sort() : [],
        scene: document.getElementById('scene-count')?.textContent,
        from: view.t0,
        to: view.t1,
      };
    }""")


def wait_scene(
    page,
    revision=0,
    selected=None,
    traced=None,
    expected_total=None,
    clock=None,
):
    try:
        page.wait_for_function("""({about, revision, selected, traced, expectedTotal, clock}) => {
          const s = globalThis.KMP_APP?.state;
          return s && s.model.about === about && s.model.total > 0 &&
            s.sync.polling && !s.sync.applying && s.sync.revision >= revision &&
            document.getElementById('stage')?.dataset.loading === 'false' &&
            document.getElementById('scene-empty')?.hidden &&
            (expectedTotal == null || s.model.total === expectedTotal) &&
            (!clock || s.view.clock === clock) &&
            (!selected || s.view.selectedRef === selected) &&
            (!traced || s.view.trace?.refs.has(traced));
        }""", arg={"about": ABOUT, "revision": revision, "selected": selected,
                     "traced": traced, "expectedTotal": expected_total,
                     "clock": clock}, timeout=20000)
    except Exception as error:
        diagnostic = page.evaluate("""() => ({
          ready: document.readyState,
          stage_loading: document.getElementById('stage')?.dataset.loading,
          scene_empty_hidden: document.getElementById('scene-empty')?.hidden,
          error: document.getElementById('error')?.textContent,
          state: globalThis.KMP_APP ? {
            about: KMP_APP.state.model.about,
            total: KMP_APP.state.model.total,
            polling: KMP_APP.state.sync.polling,
            applying: KMP_APP.state.sync.applying,
            revision: KMP_APP.state.sync.revision,
            selected: KMP_APP.state.view.selectedRef,
            trace: KMP_APP.state.view.trace ? [...KMP_APP.state.view.trace.refs] : [],
          } : null,
        })""")
        expected = {"revision": revision, "selected": selected,
                    "traced": traced, "total": expected_total, "clock": clock}
        raise RuntimeError(
            f"scene did not settle: expected={json.dumps(expected)} actual={json.dumps(diagnostic)}"
        ) from error
    page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")
    page.wait_for_timeout(75)


class NetworkTrace:
    def __init__(self, page):
        self.started = []
        self.completed = []
        self.failed = []
        page.on("request", self._request)
        page.on("requestfinished", self._finished)
        page.on("requestfailed", self._failed)

    def _request(self, request):
        if request.url.startswith("http://127.0.0.1:"):
            self.started.append({"url": public_url(request.url), "method": request.method,
                                 "at_ns": time.perf_counter_ns()})

    def _finished(self, request):
        if request.url.startswith("http://127.0.0.1:"):
            self.completed.append({"url": public_url(request.url),
                                   "timing": request.timing, "sizes": request.sizes()})

    def _failed(self, request):
        if request.url.startswith("http://127.0.0.1:"):
            self.failed.append({"url": public_url(request.url),
                                "failure": request.failure})

    def mark(self):
        return len(self.started), len(self.completed), len(self.failed)

    def phase(self, mark):
        started, completed, failed = mark
        rows = self.started[started:]
        done = self.completed[completed:]
        return {
            "http_requests_started": len(rows),
            "request_paths": [urlsplit(row["url"]).path for row in rows],
            "http_completed": done,
            "http_response_bytes_completed": sum(
                row["sizes"]["responseBodySize"] + row["sizes"]["responseHeadersSize"]
                for row in done
            ),
            "http_failed": self.failed[failed:],
        }


def timed_scene(operation, sample, network, action, page, **wait):
    mark = network.mark()
    started = time.perf_counter_ns()
    rpc = action()
    wait_scene(page, **wait)
    result = {"operation": operation, "sample": sample,
              "elapsed_ms": (time.perf_counter_ns() - started) / 1e6,
              "state": scene_state(page), **network.phase(mark)}
    if rpc:
        result["mcp"] = rpc
    return result


def one_sample(browser, side, binary, scratch, sample):
    packet, refs = fixture()
    client = Client(binary, scratch / f"{side}-{sample:02}")
    try:
        client.initialize()
        client.call("kmp_ingest", packet)
        url = client.viewer_url()
        context = browser.new_context(viewport={"width": 1440, "height": 900})
        page = context.new_page()
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("console", lambda message: errors.append(message.text)
                if message.type == "error" else None)
        network = NetworkTrace(page)

        def open_action():
            opened, metrics = client.call("kmp_view_open", {"view_id": "default", "about": ABOUT})
            state = structured(opened)
            if not state.get("viewer_available"):
                raise AssertionError(state)
            page.goto(state.get("url", url), wait_until="domcontentloaded")
            return metrics

        cold = timed_scene(
            "agent-open", sample, network, open_action, page,
            expected_total=32, clock="occurred",
        )
        cold["server_resources"] = client.resources()

        def focus_action():
            current, get_metrics = client.call("kmp_view_get_state", {"view_id": "default"})
            state = structured(current)
            selected = refs[2]
            axis = ("occurred", "observed", "ingested")[sample % 3]
            focus_action.axis = axis
            applied, apply_metrics = client.call("kmp_view_apply_intent", {
                "view_id": "default",
                "expected_revision": state["view_revision"],
                "idempotency_key": f"performance-777-focus-{side}-{sample}",
                "selection": selected,
                "focus": {
                    "refs": [selected, refs[0]],
                    "time_range": {"axis": axis},
                },
                "trace": {"from": selected, "to": refs[0]},
                "projection": {"dimensions": ["task"]},
            })
            intent = structured(applied)
            if not intent.get("applied"):
                raise AssertionError(intent)
            focus_action.revision = intent["view_revision"]
            return {"get_state": get_metrics, "apply_intent": apply_metrics}

        focus_action.revision = 0
        focus_action.axis = None
        mark = network.mark()
        started = time.perf_counter_ns()
        mcp = focus_action()
        wait_scene(
            page, revision=focus_action.revision, selected=refs[2],
            traced=refs[0], clock=focus_action.axis,
        )
        focus = {"operation": "agent-focus-trace", "sample": sample,
                 "elapsed_ms": (time.perf_counter_ns() - started) / 1e6,
                 "state": scene_state(page), "mcp": mcp, **network.phase(mark)}
        focus["server_resources"] = client.resources()

        def reload_action():
            page.reload(wait_until="domcontentloaded")

        warm = timed_scene(
            "browser-reload-rich-state", sample, network,
            reload_action, page,
            revision=focus_action.revision, selected=refs[2], traced=refs[0],
            clock=focus_action.axis,
        )
        warm["server_resources"] = client.resources()
        context.close()
        if errors:
            raise AssertionError(errors)
        return {"side": side, "sample": sample,
                "operations": [cold, focus, warm], "browser_errors": errors}
    finally:
        client.close()


def summary(rows):
    grouped = defaultdict(list)
    for row in rows:
        for operation in row["operations"]:
            grouped[(row["side"], operation["operation"])].append(operation)
    result = {}
    for (side, name), samples in grouped.items():
        elapsed = sorted(row["elapsed_ms"] for row in samples)
        result.setdefault(side, {})[name] = {
            "samples": len(samples),
            "p50_ms": statistics.median(elapsed),
            "p95_ms": elapsed[math.ceil(len(elapsed) * .95) - 1],
            "http_requests_started": sorted({row["http_requests_started"] for row in samples}),
            "http_response_bytes_completed": sorted({row["http_response_bytes_completed"] for row in samples}),
            "server_peak_rss_kib": [row["server_resources"]["peak_rss_kib"] for row in samples],
            "server_cpu_ticks": [row["server_resources"]["cpu_ticks"] for row in samples],
            "request_path_counts": [{path: row["request_paths"].count(path)
                                     for path in sorted(set(row["request_paths"]))}
                                    for row in samples],
        }
    return result


def parity(rows):
    paired = defaultdict(dict)
    for row in rows:
        for operation in row["operations"]:
            state = dict(operation["state"])
            state.pop("sync_revision", None)
            paired[(row["sample"], operation["operation"])][row["side"]] = state
    mismatches = []
    for key, sides in sorted(paired.items()):
        if sides.get("baseline") != sides.get("candidate"):
            mismatches.append({"sample": key[0], "operation": key[1], **sides})
    return {"pairs": len(paired), "mismatches": mismatches, "exact": not mismatches}


def cpu_model():
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.lower().startswith(("model name", "hardware")) and ":" in line:
            return line.split(":", 1)[1].strip()
    return platform.processor() or "unreported"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("scratch", type=Path)
    parser.add_argument("--samples", type=int, default=8)
    args = parser.parse_args()
    for name in ("baseline", "candidate", "output", "scratch"):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    args.scratch.mkdir(parents=True, exist_ok=False)
    environment = {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "baseline_sha256": sha(args.baseline),
        "candidate_sha256": sha(args.candidate),
        "baseline_size_bytes": args.baseline.stat().st_size,
        "candidate_size_bytes": args.candidate.stat().st_size,
        "runner_sha256": sha(Path(__file__)),
        "profile": "Cargo dev; workspace .cargo/config.toml; no profile overrides",
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "platform": platform.platform(), "machine": platform.machine(),
        "cpu": cpu_model(), "cpu_count": os.cpu_count(),
        "samples_per_side": args.samples, "concurrency": 1,
        "fixture": {"abouts": 1, "entries": 32, "dimensions": 1, "relations": 11,
                    "clocks": ["occurred", "observed", "ingested"], "expiry": None},
        "operations": {
            "agent-open": "kmp_view_open plus first real HTTP scene in a fresh process/context",
            "agent-focus-trace": "get state plus apply intent through browser revision, selected occurred/observed/ingested axis, selection and trace adoption",
            "browser-reload-rich-state": "reload in the same context after focus/trace, preserving authoritative state",
        },
        "ready_observable": "stage loading=false, nonempty scene, sync polling/applying settled at expected revision, exact selection/trace where applicable, then two animation frames and 75ms request completion grace",
        "timing_instrumentation": "uninstrumented kmp-mcp and Chromium wall time",
        "allocation_scope": "not measured by this runner",
        "server_cost": "uninstrumented process current/peak RSS in KiB and cumulative Linux clock ticks after each settled operation",
        "cold_definition": "fresh embedded store, server process and browser context; OS page cache uncontrolled",
        "warm_definition": "same server process and browser context after authoritative focus/trace state",
        "latency_note": "informational local wall time; publish only from an exclusive quiet slot",
    }
    dump(args.output / "environment.json", environment)
    rows = []
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(headless=True, args=["--enable-unsafe-swiftshader"])
            environment["browser"] = browser.version
            for side, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
                for sample in range(args.samples):
                    rows.append(one_sample(browser, side, binary, args.scratch, sample))
                    print(f"{side} {sample + 1}/{args.samples}", flush=True)
            browser.close()
        compared = parity(rows)
        waterfall = args.output / "waterfalls.json.gz"
        dump_gzip(waterfall, rows)
        raw_bytes = len((json.dumps(rows, ensure_ascii=False, indent=2) + "\n").encode())
        dump(args.output / "compressed-manifest.json", {
            "waterfalls.json": {
                "original_bytes": raw_bytes,
                "compressed_file": waterfall.name,
                "compressed_bytes": waterfall.stat().st_size,
                "compressed_sha256": sha(waterfall),
            },
        })
        dump(args.output / "results.json", {
            "summary": summary(rows),
            "parity": compared,
        })
        dump(args.output / "environment.json", environment)
        if not compared["exact"]:
            raise AssertionError("baseline/candidate scene parity failed")
    finally:
        shutil.rmtree(args.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
