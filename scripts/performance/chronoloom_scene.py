#!/usr/bin/env python3
"""Record real Chromium scene updates, long tasks, heap and resource lifetime.

Requires Python Playwright 1.58 and its Chromium. Run against the requested
checkout before/after changes; no server, model or personal store is involved.
Usage: python scripts/performance/chronoloom_scene.py NEW_OUTPUT [--entries 2048]
"""
import argparse
import json
import hashlib
import platform
import subprocess
from pathlib import Path
from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source-ref", help="Read renderer modules from an existing Git revision for a baseline")
    parser.add_argument("--entries", type=int, default=2048)
    parser.add_argument("--degree", type=int, default=2)
    parser.add_argument("--samples", type=int, default=8)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    ui = root / "crates/kmp-viewer/ui"
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    names = ["vendor/three.min.js", "loom-core.js", "loom-camera.js", "loom-scene-model.js", "loom-planes.js", "loom-relation-lines.js", "loom-three.js"]
    source_dir = out / "sources"
    sources = {}
    for name in names:
        if args.source_ref:
            read = subprocess.run(["git", "show", f"{args.source_ref}:crates/kmp-viewer/ui/{name}"], cwd=root, capture_output=True)
            if read.returncode:
                if name in ("loom-planes.js", "loom-relation-lines.js"):
                    continue
                raise RuntimeError(read.stderr.decode())
            source = read.stdout
        else:
            source = (ui / name).read_bytes()
        target = source_dir / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source)
        sources[name] = hashlib.sha256(source).hexdigest()
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, args=["--enable-unsafe-swiftshader"])
        page = browser.new_page(viewport={"width": 1280, "height": 900})
        page.set_content('''<style>:root{--canvas:#101c15}body{margin:0}
        #scene-world{position:relative;width:1200px;height:800px}
        canvas{width:100%;height:100%}#scene-labels{position:absolute;inset:0;pointer-events:none}
        .plane-label,.axis-label{position:absolute;color:white}.plane-label{min-width:160px}
        small{display:block}.plane-leaders{position:absolute;width:100%;height:100%}
        </style><div id="scene-world"><canvas id="canvas"></canvas><div id="scene-labels"></div></div>''')
        page.evaluate("globalThis.KMP_APP={theme:{classColor:()=> '#aabbee'}}")
        for name in sources:
            page.add_script_tag(path=str(source_dir / name))
        page.add_script_tag(path=str(Path(__file__).with_name("chronoloom_scene_control.js")))
        client = page.context.new_cdp_session(page)
        client.send("Performance.enable")
        trace = {}
        client.on("Tracing.tracingComplete", lambda data: trace.update(data))
        client.send("Tracing.start", {"categories": "devtools.timeline,blink.user_timing,v8", "transferMode": "ReturnAsStream"})
        result = page.evaluate("async opts => await sceneControl(opts)", vars(args) | {"output": str(out)})
        page.screenshot(path=str(out / "scene.png"))
        result["disposal"] = page.evaluate("globalThis.disposeSceneControl?.() || null")
        result["browser"] = browser.version
        result["platform"] = platform.platform()
        result["commit"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
        result["source_ref"] = args.source_ref
        result["sources"] = sources
        result["performance_metrics"] = client.send("Performance.getMetrics")
        client.send("Tracing.end")
        while "stream" not in trace:
            page.wait_for_timeout(100)
        with (out / "chromium-trace.json").open("w") as f:
            while True:
                block = client.send("IO.read", {"handle": trace["stream"]})
                assert not block.get("base64Encoded"), "unexpected binary trace"
                f.write(block["data"])
                if block.get("eof"):
                    break
        client.send("IO.close", {"handle": trace["stream"]})
        page.add_script_tag(path=str(Path(__file__).with_name("chronoloom_scene_checks.js")))
        result["behavior"] = page.evaluate("async () => KMP_APP.planes ? await checkSceneBehavior() : null")
        (out / "results.json").write_text(json.dumps(result, indent=2) + "\n")
        browser.close()


if __name__ == "__main__":
    main()
