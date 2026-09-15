#!/usr/bin/env python3
"""Audit pagination arithmetic and continuations in #539 batch captures.

Usage: proof_batch_capture_audit.py ARTIFACT_DIR [OUTPUT_JSON]
"""
from __future__ import annotations

import gzip
import hashlib
import json
from pathlib import Path
import sys


SECTIONS = ("trace", "objects", "supports", "gaps")


def sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def audit(path: Path) -> dict:
    operations = []
    current = None
    with gzip.open(path, "rt") as rows:
        for line in rows:
            row = json.loads(line)
            request = row["request"]
            if request.get("method") != "tools/call":
                continue
            params = request["params"]
            if params.get("name") != "kmp_trace":
                continue
            arguments = params["arguments"]
            response = row["response"]["result"]["structuredContent"]
            cursor = arguments.get("page", {}).get("cursor")
            if cursor is None:
                assert current is None, "a new operation started before the prior one ended"
                current = {
                    "offset": 0,
                    "total": None,
                    "pages": 0,
                    "next_arguments": None,
                    "from": arguments["from"],
                    "trace": [],
                    "objects": [],
                    "supports": [],
                    "gaps": [],
                }
            else:
                assert current is not None, "capture starts in the middle of an operation"
                assert arguments == current["next_arguments"], "request differs from offered continuation"

            page = response["page"]
            returned = sum(len(response[name]) for name in SECTIONS)
            assert page["returned"] == returned, "page.returned does not match typed sections"
            assert page["offset"] == current["offset"], "page offsets are not contiguous"
            if current["total"] is None:
                current["total"] = page["total"]
            assert page["total"] == current["total"], "page.total changed within one operation"
            assert page["offset"] + returned <= page["total"], "page exceeds total"
            current["offset"] += returned
            current["pages"] += 1
            for name in SECTIONS:
                current[name].extend(response[name])

            actions = [action for action in response["next_actions"] if action["tool"] == "kmp_trace"]
            if page["has_more"]:
                assert page["next_cursor"], "partial page omitted its cursor"
                matching = [
                    action for action in actions
                    if action["arguments"].get("page", {}).get("cursor") == page["next_cursor"]
                ]
                assert len(matching) == 1, "partial page must offer one matching continuation"
                current["next_arguments"] = matching[0]["arguments"]
            else:
                assert page["next_cursor"] in (None, ""), "complete page retained a cursor"
                assert not actions, "complete page retained a Trace continuation"
                assert current["offset"] == current["total"], "final page did not reach total"
                object_refs = [item["ref"] for item in current["objects"]]
                assert len(object_refs) == len(set(object_refs)), "proof objects contain duplicate refs"
                selected = {current["from"]}
                for relation in current["trace"]:
                    selected.update([relation["from"], relation["to"]])
                sources = {relation["from"] for relation in current["supports"]}
                assert set(object_refs) == selected | sources, "proof object set is incomplete or extraneous"
                assert current["gaps"] == [], "complete fixture returned proof gaps"
                operations.append({"pages": current["pages"], "total": current["total"]})
                current = None
    assert current is None, "capture ended in the middle of an operation"
    assert operations, "capture contains no Trace operations"
    return {
        "capture": path.name,
        "capture_sha256": sha(path),
        "operations": len(operations),
        "page_counts": sorted({row["pages"] for row in operations}),
        "totals": sorted({row["total"] for row in operations}),
    }


def main() -> None:
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    artifact_dir = Path(sys.argv[1]).resolve()
    captures = [audit(path) for path in sorted(artifact_dir.glob("*-batch.jsonl.gz"))]
    auditor = Path(__file__).resolve()
    result = {
        "contract": "kmp.proof-batch-capture-audit.v1",
        "auditor": "scripts/performance/proof_batch_capture_audit.py",
        "auditor_sha256": sha(auditor),
        "captures": captures,
    }
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if len(sys.argv) == 3:
        Path(sys.argv[2]).write_text(encoded)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
