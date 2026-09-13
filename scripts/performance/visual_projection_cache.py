#!/usr/bin/env python3
"""#779 native projection comparison; stores are synthetic and disposable.

Build: cargo build --locked -p kmp-adapter-embedded --example visual_projection_benchmark
Run: python3 scripts/performance/visual_projection_cache.py artifacts/performance-779/run
Evidence directories must be new. First reads exclude fixture setup; these are
new-process reads with uncontrolled OS cache, never claimed to be OS-cold.
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
    parser.add_argument("output", type=Path)
    parser.add_argument("--samples", type=int, default=20)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    binary = root / "target/debug/examples/visual_projection_benchmark"
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (root / "tmp").mkdir(exist_ok=True)
    info = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "profile": "dev, shared workspace Cargo configuration",
            "platform": platform.platform(), "machine": platform.machine(),
            "cpu_count": os.cpu_count(), "concurrency": 1,
            "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
            "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
            "samples_per_process": args.samples,
            "baseline": "Same application and snapshot; adapter declines cache revision, so every projection is recomputed",
            "startup": "Fixture creation and process startup excluded from read timing, included in process RSS"}
    (out / "environment.json").write_text(json.dumps(info, indent=2) + "\n")
    (out / "source.patch").write_bytes(subprocess.check_output(["git", "diff", "HEAD"], cwd=root))
    files = list((root / "crates/kmp-application/src/memory").glob("visual_projection*.rs"))
    files += [root / "crates/kmp-application/src/memory/service.rs",
              root / "crates/kmp-adapter-embedded/examples/visual_projection_benchmark.rs"]
    (out / "source-hashes.json").write_text(json.dumps({str(f.relative_to(root)):
        hashlib.sha256(f.read_bytes()).hexdigest() for f in files}, indent=2) + "\n")
    with tempfile.TemporaryDirectory(prefix="projection-control-", dir=root / "tmp") as scratch:
        env = dict(os.environ, TMPDIR=scratch)
        for shape in ["small", "medium", "dense", "large_body"]:
            with (out / f"{shape}-equivalence.json").open("w") as f:
                subprocess.run([str(binary), "verify", shape, "1"], env=env, stdout=f, check=True)
            for i, mode in enumerate(["baseline", "cache", "cache", "baseline"]):
                name = f"{shape}-{i+1:02}-{mode}"
                with (out / f"{name}.json").open("w") as f:
                    subprocess.run(["/usr/bin/time", "-v", "-o", str(out / f"{name}.time"),
                        str(binary), mode, shape, str(args.samples)], env=env, stdout=f, check=True)
                print(name + " complete", flush=True)


if __name__ == "__main__":
    main()
