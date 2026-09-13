#!/usr/bin/env python3
"""Paired real-browser and MCP App asset measurements for #776.

Usage: chronoloom_assets.py BASELINE CANDIDATE COUNTER OUTPUT SCRATCH [--samples 8]

Both binaries must use the same Cargo profile. Stores are synthetic and
disposable. Browser first-scene ends only after ChronoLoom has completed its
real HTTP projection and drawn a non-empty scene. Native allocation counts are
cumulative requests/bytes from LD_PRELOAD, not retained heap or live bytes.
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
from pathlib import Path

from playwright.sync_api import sync_playwright


VIEWER_URL = re.compile(r"memory viewer at (http://[^;\s]+)")
ALLOCATION = '{"native_allocation_control":true,'


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
    about = "project:chronoloom-assets"
    entries = []
    for index in range(32):
        entries.append({
            "id": f"{about}:observation:item-{index:02}",
            "kind": "observation",
            "text": f"Synthetic browser memory {index}",
            "coordinates": [{
                "dimension": "timeline",
                "scope_id": "timeline:browser",
                "occurred_at": f"2026-09-01T10:00:{index:02}Z",
                "observed_at": f"2026-09-01T10:00:{index:02}Z",
                "ingested_at": f"2026-09-01T10:00:{index:02}Z",
                "sequence": index + 1,
            }],
        })
    return {
        "about": about,
        "idempotency_key": "performance-776-browser-v1",
        "memory": {
            "dimensions": [{"id": "timeline:browser", "kind": "timeline"}],
            "entries": entries,
        },
    }


class Client:
    def __init__(self, binary, store, counter=None, viewer=True):
        store.mkdir(parents=True, exist_ok=False)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("KMP_", "KERNEL_"))}
        env.update(KMP_MCP_BACKEND="embedded", KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR="127.0.0.1:0" if viewer else "off",
                   XDG_DATA_HOME=str(store / "xdg"), XDG_STATE_HOME=str(store / "state"))
        if counter:
            env["LD_PRELOAD"] = str(counter)
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1,
            env=env, cwd=store)
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

    def resources(self):
        status = {}
        for line in Path(f"/proc/{self.process.pid}/status").read_text().splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                status[key] = value.strip()
        return {"peak_rss_kib": int(status["VmHWM"].split()[0]),
                "rss_kib": int(status["VmRSS"].split()[0])}

    def rpc(self, method, params):
        self.serial += 1
        request = {"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params}
        before = self.resources()
        started = time.perf_counter_ns()
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        elapsed_ms = (time.perf_counter_ns() - started) / 1e6
        if not line:
            raise RuntimeError("MCP process closed before answering: " + "".join(self.stderr))
        response = json.loads(line)
        if response.get("id") != self.serial or "error" in response:
            raise RuntimeError(json.dumps(response))
        result = response["result"]
        if result.get("isError"):
            raise RuntimeError(json.dumps(result))
        return result, {"elapsed_ms": elapsed_ms, "response_bytes": len(line.encode()),
                        "before": before, "after": self.resources()}

    def initialize(self, apps=False):
        capabilities = {}
        if apps:
            capabilities = {"extensions": {"io.modelcontextprotocol/ui": {
                "mimeTypes": ["text/html;profile=mcp-app"]}}}
        return self.rpc("initialize", {"protocolVersion": "2024-11-05",
            "capabilities": capabilities,
            "clientInfo": {"name": "performance-776", "version": "1"}})

    def call(self, tool, arguments):
        return self.rpc("tools/call", {"name": tool, "arguments": arguments})

    def viewer_url(self):
        return self.viewer_urls.get(timeout=30)

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
        counts = [json.loads(line) for line in self.stderr
                  if line.startswith(ALLOCATION)]
        return counts[0] if counts else None


def summarize(values):
    ordered = sorted(values)
    return {"samples": len(values), "p50": statistics.median(values),
            "p95": ordered[min(len(ordered) - 1, max(0, math.ceil(len(ordered) * .95) - 1))],
            "min": ordered[0], "max": ordered[-1]}


def cpu_model():
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.lower().startswith(("model name", "hardware")) and ":" in line:
            return line.split(":", 1)[1].strip()
    return platform.processor() or "unreported"


def browser_load(page, url, phase):
    requests = []
    current = {}
    connections = set()

    def requested(event):
        requested_url = event["request"]["url"]
        if not requested_url.startswith("http://127.0.0.1:"):
            return
        if event.get("redirectResponse") and event["requestId"] in current:
            redirected = event["redirectResponse"]
            row = requests[current[event["requestId"]]]
            row.update(status=redirected["status"],
                       headers={key.lower(): value for key, value in redirected["headers"].items()},
                       encoded_bytes=redirected.get("encodedDataLength", 0))
            if redirected.get("connectionId") is not None:
                connections.add(redirected["connectionId"])
        requests.append({"url": requested_url})
        current[event["requestId"]] = len(requests) - 1

    def responded(event):
        if event["requestId"] not in current:
            return
        response = event["response"]
        row = requests[current[event["requestId"]]]
        row.update(status=response["status"], mime_type=response.get("mimeType"),
                   headers={key.lower(): value for key, value in response["headers"].items()},
                   from_disk_cache=response.get("fromDiskCache", False),
                   from_service_worker=response.get("fromServiceWorker", False))
        if response.get("connectionId") is not None:
            connections.add(response["connectionId"])

    def finished(event):
        if event["requestId"] in current:
            requests[current[event["requestId"]]]["encoded_bytes"] = event.get("encodedDataLength", 0)

    cdp = page.context.new_cdp_session(page)
    cdp.send("Network.enable")
    cdp.on("Network.requestWillBeSent", requested)
    cdp.on("Network.responseReceived", responded)
    cdp.on("Network.loadingFinished", finished)
    started = time.perf_counter_ns()
    if phase == "cold":
        page.goto(url, wait_until="domcontentloaded")
    else:
        page.reload(wait_until="domcontentloaded")
    page.wait_for_function("""() => document.getElementById('stage')?.dataset.loading === 'false'
        && document.getElementById('scene-count')?.textContent === '1 about · 32 memories'
        && KMP_APP.state.model.about === 'project:chronoloom-assets'
        && KMP_APP.state.model.currentLod === 'moment'
        && KMP_APP.state.model.total === 32
        && KMP_APP.state.model.entries.length === 32
        && KMP_APP.state.view.selectedRef === null
        && KMP_APP.state.sync.applying === false && KMP_APP.state.sync.revision > 0""",
        timeout=60000)
    page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")
    page.wait_for_timeout(100)
    first_scene_ms = (time.perf_counter_ns() - started) / 1e6
    scene = page.evaluate("""() => ({
        text: document.getElementById('scene-count')?.textContent,
        about: KMP_APP.state.model.about,
        lod: KMP_APP.state.model.currentLod,
        selection_total: KMP_APP.state.model.total,
        projected_entries: KMP_APP.state.model.entries.length,
        selected_ref: KMP_APP.state.view.selectedRef,
        sync_revision: KMP_APP.state.sync.revision,
    })""")
    cdp.send("Network.disable")
    rows = requests
    static = [row for row in rows if "/assets/" in row["url"]]
    dynamic = [row for row in rows if "/assets/" not in row["url"]]
    headers = [{"path": row["url"].split("?", 1)[0].split("/", 3)[-1],
                "cache_control": row.get("headers", {}).get("cache-control"),
                "content_encoding": row.get("headers", {}).get("content-encoding"),
                "vary": row.get("headers", {}).get("vary"),
                "connection": row.get("headers", {}).get("connection")}
               for row in rows]
    return {"phase": phase, "first_scene_ms": first_scene_ms,
            "requests": len(rows), "static_requests": len(static),
            "dynamic_requests": len(dynamic), "connections": len(connections),
            "encoded_bytes": sum(row.get("encoded_bytes", 0) for row in rows),
            "static_encoded_bytes": sum(row.get("encoded_bytes", 0) for row in static),
            "dynamic_encoded_bytes": sum(row.get("encoded_bytes", 0) for row in dynamic),
            "disk_cache_hits": sum(row.get("from_disk_cache", False) for row in rows),
            "headers": headers, "scene": scene}


def browser_samples(browser, side, binary, counter, scratch, samples, label):
    rows = []
    for sample in range(samples):
        client = Client(binary, scratch / f"browser-{label}-{side}-{sample:02}", counter=counter)
        try:
            client.initialize()
            client.call("kmp_ingest", fixture())
            url = client.viewer_url()
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = context.new_page()
            cold = browser_load(page, url, "cold")
            warm = browser_load(page, url, "warm")
            rss = client.resources()
            context.close()
        finally:
            allocations = client.close()
        rows.append({"side": side, "sample": sample, "cold": cold, "warm": warm,
                     "rss": rss, "allocations": allocations})
        print(f"{side} browser {label} {sample + 1}/{samples}", flush=True)
    return rows


def mcp_resource_samples(side, binary, counter, scratch, samples, label):
    rows = []
    for sample in range(samples):
        client = Client(binary, scratch / f"mcp-{label}-{side}-{sample:02}", counter=counter,
                        viewer=False)
        try:
            client.initialize(apps=True)
            reads = []
            bodies = []
            for _ in range(2):
                result, metrics = client.rpc("resources/read", {"uri": "ui://kmp/chronoloom.html"})
                body = result["contents"][0]["text"]
                bodies.append(hashlib.sha256(body.encode()).hexdigest())
                reads.append(metrics)
            rss = client.resources()
        finally:
            allocations = client.close()
        if len(set(bodies)) != 1:
            raise AssertionError("MCP resource changed between identical reads")
        rows.append({"side": side, "sample": sample, "reads": reads,
                     "body_sha256": bodies[0], "rss": rss, "allocations": allocations})
        print(f"{side} MCP resource {label} {sample + 1}/{samples}", flush=True)
    return rows


def aggregate(browser_rows, browser_allocations, mcp_rows, mcp_allocations):
    result = {}
    for side in ("baseline", "candidate"):
        selected = [row for row in browser_rows if row["side"] == side]
        browser_alloc = [row for row in browser_allocations if row["side"] == side]
        mcp = [row for row in mcp_rows if row["side"] == side]
        mcp_alloc = [row for row in mcp_allocations if row["side"] == side]
        result[side] = {
            "browser": {phase: {
                key: summarize([row[phase][key] for row in selected])
                for key in ("first_scene_ms", "requests", "static_requests", "dynamic_requests",
                            "connections", "encoded_bytes", "static_encoded_bytes",
                            "dynamic_encoded_bytes", "disk_cache_hits")}
                for phase in ("cold", "warm")},
            "browser_peak_rss_kib": summarize([row["rss"]["peak_rss_kib"] for row in browser_alloc]),
            "browser_allocation_requests": summarize([row["allocations"]["requests"] for row in browser_alloc]),
            "browser_requested_allocation_bytes": summarize([
                row["allocations"]["requested_bytes"] for row in browser_alloc]),
            "mcp_first_read_ms": summarize([row["reads"][0]["elapsed_ms"] for row in mcp]),
            "mcp_warm_read_ms": summarize([row["reads"][1]["elapsed_ms"] for row in mcp]),
            "mcp_response_bytes": summarize([row["reads"][0]["response_bytes"] for row in mcp]),
            "mcp_peak_rss_kib": summarize([row["rss"]["peak_rss_kib"] for row in mcp_alloc]),
            "mcp_allocation_requests": summarize([row["allocations"]["requests"] for row in mcp_alloc]),
            "mcp_requested_allocation_bytes": summarize([
                row["allocations"]["requested_bytes"] for row in mcp_alloc]),
            "mcp_body_sha256": sorted({row["body_sha256"] for row in mcp}),
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("counter", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("scratch", type=Path)
    parser.add_argument("--samples", type=int, default=8)
    args = parser.parse_args()
    for key in ("baseline", "candidate", "counter"):
        setattr(args, key, getattr(args, key).resolve())
    args.output = args.output.resolve()
    args.scratch = args.scratch.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    args.scratch.mkdir(parents=True, exist_ok=False)
    environment = {"commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "baseline_sha256": sha(args.baseline), "candidate_sha256": sha(args.candidate),
        "counter_sha256": sha(args.counter), "runner_sha256": sha(Path(__file__)),
        "profile": "Cargo dev; shared workspace configuration; no profile overrides",
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "platform": platform.platform(), "machine": platform.machine(), "cpu": cpu_model(),
        "cpu_count": os.cpu_count(),
        "samples_per_side": args.samples, "concurrency": 1,
        "synthetic_shape": {"abouts": 1, "entries": 32, "dimensions": 1,
                            "relations": 0, "entry_body": "Synthetic browser memory <index>"},
        "browser_observable": "stage data-loading=false; exact 1 about / 32 memory scene, moment projection, selection total 32, 32 projected entries, no selected ref; shared-view sync settled at revision > 0; then two animation frames",
        "encoded_bytes": "Chrome DevTools Network.loadingFinished encodedDataLength; includes HTTP framing",
        "allocation_scope": "separate instrumented kmp-mcp processes including startup, synthetic ingest, cold and warm scene or two MCP resource reads",
        "allocation_units": "allocation requests and cumulative requested bytes; not retained/live bytes",
        "timing_instrumentation": "browser and MCP latency samples are uninstrumented; LD_PRELOAD allocation-control timing is not interpreted",
        "cold_definition": "fresh browser context and new server process; OS page cache uncontrolled",
        "warm_definition": "reload in the same browser context and server process",
        "latency_note": "informational local wall time; requires a quiet measurement slot"}
    dump(args.output / "environment.json", environment)
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(headless=True, args=["--enable-unsafe-swiftshader"])
            browser_rows = []
            for side, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
                browser_rows.extend(browser_samples(browser, side, binary, None,
                                                    args.scratch, args.samples, "timing"))
            browser_allocations = []
            for side, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
                browser_allocations.extend(browser_samples(browser, side, binary, args.counter,
                                                           args.scratch, args.samples,
                                                           "allocations"))
            environment["browser"] = browser.version
            browser.close()
        mcp_rows = []
        for side, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
            mcp_rows.extend(mcp_resource_samples(side, binary, None,
                                                  args.scratch, args.samples, "timing"))
        mcp_allocations = []
        for side, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
            mcp_allocations.extend(mcp_resource_samples(side, binary, args.counter,
                                                         args.scratch, args.samples,
                                                         "allocations"))
        dump_gzip(args.output / "browser-raw.json.gz", browser_rows)
        dump_gzip(args.output / "browser-allocations.json.gz", browser_allocations)
        dump_gzip(args.output / "mcp-resource-raw.json.gz", mcp_rows)
        dump_gzip(args.output / "mcp-resource-allocations.json.gz", mcp_allocations)
        dump(args.output / "results.json", aggregate(browser_rows, browser_allocations,
                                                       mcp_rows, mcp_allocations))
        dump(args.output / "environment.json", environment)
    finally:
        shutil.rmtree(args.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
