"""Real MCP -> HTTP -> Chromium journeys; no injected replacement application."""
import time
import math
import statistics
from collections import defaultdict
from urllib.parse import urlsplit, urlunsplit, parse_qsl, urlencode


def public_url(url):
    parts = urlsplit(url)
    query = urlencode([(k, "REDACTED" if k == "k" else v)
                       for k, v in parse_qsl(parts.query)])
    return urlunsplit((parts.scheme, parts.netloc, parts.path, query, ""))


def scene_state(page):
    return page.evaluate("""() => {
      const {model, view, sync} = KMP_APP.state;
      return {about:model.about, total:model.total, lod:model.currentLod,
        revision:sync.revision, clock:view.clock, selected:view.selectedRef,
        trace: view.trace ? [...view.trace.refs].sort() : [],
        scene:document.getElementById('scene-count').textContent,
        from:view.t0, to:view.t1};
    }""")


def wait_scene(page, about, revision=0, selected=None, traced=None):
    page.wait_for_function("""({about, revision, selected, traced}) => {
      const s=globalThis.KMP_APP?.state;
      return s && s.model.about===about && s.model.total>0 &&
        s.sync.polling && !s.sync.applying && s.sync.revision>=revision &&
        document.getElementById('stage')?.dataset.loading === 'false' &&
        document.getElementById('scene-empty')?.hidden &&
        (!selected || s.view.selectedRef===selected) &&
        (!traced || s.view.trace?.refs.has(traced));
    }""", arg=dict(about=about, revision=revision, selected=selected, traced=traced), timeout=60000)
    page.evaluate("() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))")


def journey(browser, client, about, refs, samples, out):
    rows, requests, completed, errors = [], [], [], []
    with browser.new_context(viewport={"width": 1440, "height": 900}) as context:
        page = context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("request", lambda request: requests.append({"url": public_url(request.url),
            "method": request.method, "at_ns": time.perf_counter_ns()}))
        page.on("requestfinished", lambda request: completed.append({
            "url": public_url(request.url), "timing": request.timing,
            "sizes": request.sizes()}))
        cdp = context.new_cdp_session(page)
        cdp.send("Performance.enable")
        for i in range(samples + 1):
            request_start, complete_start = len(requests), len(completed)
            start = time.perf_counter_ns()
            if i == 0:
                opened, rpc = client.call("kmp_view_open", {"view_id": "default", "about": about})
                state = opened["structuredContent"]
                url = state["url"]
                assert state["viewer_available"], state
                page.goto(url, wait_until="domcontentloaded")
            else:
                page.reload(wait_until="domcontentloaded")
                rpc = None
            wait_scene(page, about)
            elapsed = (time.perf_counter_ns() - start) / 1e6
            rows.append({"operation": "agent-open" if i == 0 else "browser-reload",
                "sample": i, "cache": "fresh-browser-context" if i == 0 else "warm-browser-cache",
                "elapsed_ms": elapsed, "mcp_calls": int(i == 0),
                "http_requests_started": len(requests) - request_start,
                "http_completed": completed[complete_start:], "state": scene_state(page),
                "browser_metrics": cdp.send("Performance.getMetrics")["metrics"],
                "resources": page.evaluate("performance.getEntriesByType('resource').map(r=>({name:r.name.split('?')[0],startTime:r.startTime,duration:r.duration,transferSize:r.transferSize,encodedBodySize:r.encodedBodySize,decodedBodySize:r.decodedBodySize}))")})
        page.screenshot(path=str(out / "first-scene.png"))
        for i in range(samples):
            request_start, complete_start = len(requests), len(completed)
            start = time.perf_counter_ns()
            state, _ = client.call("kmp_view_get_state", {"view_id": "default"})
            selected = refs[2 + i % 2]
            result, _ = client.call("kmp_view_apply_intent", {
                "view_id": "default", "expected_revision": state["structuredContent"]["view_revision"],
                "idempotency_key": f"baseline-journey-{i}", "selection": selected,
                "focus": {"refs": [selected, refs[0]]},
                "trace": {"from": selected, "to": refs[0]}})
            intent = result["structuredContent"]
            assert intent["applied"], intent
            wait_scene(page, about, intent["view_revision"], selected, refs[0])
            rows.append({"operation": "agent-focus-trace", "sample": i,
                "elapsed_ms": (time.perf_counter_ns() - start) / 1e6,
                "mcp_calls": 2,
                "http_requests_started": len(requests) - request_start,
                "http_completed": completed[complete_start:], "state": scene_state(page)})
        assert not errors, errors
        grouped = defaultdict(list)
        for row in rows:
            grouped[row["operation"]].append(row)
        summary = {}
        for operation, samples in grouped.items():
            elapsed = sorted(row["elapsed_ms"] for row in samples)
            summary[operation] = {"samples": len(samples),
                "p50_ms": statistics.median(elapsed),
                "p95_ms": elapsed[math.ceil(len(elapsed) * .95) - 1],
                "http_requests_started": [row["http_requests_started"] for row in samples],
                "http_response_bytes_completed": [sum(
                    item["sizes"]["responseBodySize"] + item["sizes"]["responseHeadersSize"]
                    for item in row["http_completed"]) for row in samples]}
        return {"rows": rows, "summary": summary, "requests": requests, "browser_errors": errors}
