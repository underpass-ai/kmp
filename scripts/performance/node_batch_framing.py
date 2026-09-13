#!/usr/bin/env python3
"""ChronoLoom framing batch runner for #539. Synthetic stores; no model calls.

Usage: python3 scripts/performance/node_batch_framing.py OUT
Build the viewer example first with the shared Cargo configuration:
    cargo build --locked -p kmp-viewer --example node_batch_benchmark
OUT is a new directory, normally under the ignored `artifacts/`; existing
evidence is never overwritten. Disposable stores live under `tmp/` and are
removed after the run.
"""
import argparse
import hashlib
import json
import os
import platform
import subprocess
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New directory; existing evidence is never overwritten")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    binary = root / "target/debug/examples/node_batch_benchmark"
    if not binary.is_file():
        parser.error("Build kmp-viewer --example node_batch_benchmark first")
    out.mkdir(parents=True, exist_ok=False)
    (root / "tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="node-batch-benchmark-", dir=root / "tmp") as scratch:
        env = dict(os.environ, TMPDIR=scratch)
        info = {
            "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
            "profile": "dev; rebuild with the current shared Cargo configuration before running",
            "platform": platform.platform(),
            "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        }
        (out / "binary.json").write_text(json.dumps(info, indent=2) + "\n")
        with (out / "equivalence.json").open("w") as f:
            subprocess.run([str(binary), "verify"], cwd=root, env=env, stdout=f, check=True)
        # Reverse the order in the second round; retain every sample and first read.
        order = ["baseline", "batch", "baseline-http", "batch-http",
                 "batch-http", "baseline-http", "batch", "baseline"]
        for index, mode in enumerate(order):
            name = f"{index + 1:02}-{mode}"
            with (out / f"{name}.json").open("w") as f:
                subprocess.run(["/usr/bin/time", "-v", "-o", str(out / f"{name}.time"),
                                str(binary), mode, "20"],
                               cwd=root, env=env, stdout=f, check=True)
            print(name + " complete", flush=True)


if __name__ == "__main__":
    main()
