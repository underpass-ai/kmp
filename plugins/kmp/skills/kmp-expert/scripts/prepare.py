#!/usr/bin/env python3
"""Prepare a reusable complete agent guide from the shipped canonical asset."""

import argparse
import hashlib
import json
from pathlib import Path


def digest(data):
    return hashlib.sha256(data).hexdigest()


def prepare(plugin, cache):
    source = plugin / "guide" / "guide.requests.json"
    asset = source.read_bytes()
    requests = json.loads(asset)
    guide = next(item for item in requests if item["about"] == "guide:kmp-agent")
    entries = guide["memory"]["entries"]
    revisions = {entry["metadata"]["guide_revision"] for entry in entries}
    if len(revisions) != 1 or not all(revisions):
        raise ValueError("the shipped agent guide has mixed or missing revisions")
    revision = revisions.pop()
    chunks = [f"# Complete KMP agent guide\n\nGuide revision: {revision}\n\n"]
    nodes = []
    next_line = chunks[0].count("\n") + 1
    for entry in entries:
        body = entry["text"]
        section = f"## {entry['id']}\n\n{body}\n\n"
        lines = section.count("\n")
        nodes.append({
            "ref": entry["id"], "body_sha256": digest(body.encode()),
            "start_line": next_line, "end_line": next_line + lines - 1,
        })
        chunks.append(section)
        next_line += lines
    content = "".join(chunks).encode()
    manifest = {
        "asset_sha256": digest(asset), "guide_revision": revision,
        "guide_sha256": digest(content), "guide_bytes": len(content),
        "node_count": len(nodes), "nodes": nodes,
    }
    destination = cache.resolve() / manifest["asset_sha256"]
    destination.mkdir(parents=True, exist_ok=True)
    guide_path = destination / "guide.md"
    manifest_path = destination / "manifest.json"
    manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
    reused = True
    for path, expected in [(guide_path, content), (manifest_path, manifest_bytes)]:
        if not path.is_file() or path.read_bytes() != expected:
            path.write_bytes(expected)
            reused = False
    return {
        "manifest": str(manifest_path), "guide": str(guide_path),
        "guide_revision": revision, "node_count": len(nodes), "reused": reused,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", type=Path, required=True)
    args = parser.parse_args()
    plugin = Path(__file__).resolve().parents[3]
    print(json.dumps(prepare(plugin, args.cache_dir)))


if __name__ == "__main__":
    main()
