#!/usr/bin/env python3
"""Write authored memory packets into a fresh store and export its bundle.

    python3 scripts/examples/bundle.py --binary target/debug/kmp-mcp \
        --requests examples/retry-budget/requests.json \
        --out examples/retry-budget/memory.jsonl

The requests file is a JSON array of `kmp_ingest` arguments, the same shape
the shipped guide and demo use. Each packet goes through the public MCP
writer of a temporary store, exactly as an agent would send it; the exported
bundle is what `kmp-mcp import <file>` replays into an empty store. No model
is involved and nothing outside the temporary directory is written.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "guide_examples"))
from stdio import Stdio  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=Path, required=True, help="kmp-mcp executable")
    parser.add_argument("--requests", type=Path, required=True, help="JSON array of kmp_ingest arguments")
    parser.add_argument("--out", type=Path, required=True, help="bundle to write")
    args = parser.parse_args()

    binary = args.binary.resolve()
    requests = json.loads(args.requests.read_text(encoding="utf-8"))
    if not isinstance(requests, list) or not requests:
        raise SystemExit(f"{args.requests}: expected a non-empty JSON array of kmp_ingest arguments")
    abouts = sorted({request["about"] for request in requests})

    with tempfile.TemporaryDirectory(prefix="kmp-example-") as directory:
        scratch = Path(directory)
        store = scratch / "store"
        env = {key: value for key, value in os.environ.items() if not key.startswith(("KMP_", "KERNEL_"))}
        env.update(
            HOME=str(scratch / "home"),
            XDG_CONFIG_HOME=str(scratch / "home" / ".config"),
            XDG_DATA_HOME=str(scratch / "home" / ".local" / "share"),
            KMP_MCP_BACKEND="embedded",
            KMP_MCP_DATA_DIR=str(store),
            KMP_VIEWER_ADDR="off",
        )
        (scratch / "home").mkdir()
        client = Stdio(binary, scratch, env, lambda event: None)
        try:
            client.rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                      "clientInfo": {"name": "kmp-example-bundle", "version": "1"}})
            for request in requests:
                client.call("kmp_ingest", request)
        finally:
            client.close()
        export = subprocess.run(
            [str(binary), "export", str(args.out.resolve())] + [arg for about in abouts for arg in ("--about", about)],
            cwd=scratch, env=env, text=True, capture_output=True,
        )
        if export.returncode != 0:
            sys.stderr.write(export.stderr)
            return export.returncode
    header = json.loads(args.out.read_text(encoding="utf-8").splitlines()[0])
    print(f"{args.out}: {header['event_count']} events, abouts {header['abouts']}, "
          f"bundle format {header['bundle_format']}, kernel {header['kernel_version']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
