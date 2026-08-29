#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import pathlib
import sys

CAMPAIGN = pathlib.Path(__file__).resolve().parents[2]
ROOT = CAMPAIGN.parents[1]
sys.path.insert(0, str(CAMPAIGN / "scripts"))

from capture_contract import credential_findings, repo_relative


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


if len(sys.argv) != 4:
    raise SystemExit("usage: promote-run.py RUN_DIR RAW_MKV PROMOTED_INDEX")

run = pathlib.Path(sys.argv[1]).resolve()
raw = pathlib.Path(sys.argv[2]).resolve()
index = pathlib.Path(sys.argv[3]).resolve()
verification = json.loads((run / "verification.json").read_text())
if not verification.get("passed"):
    raise SystemExit("refusing to promote a capture whose verification did not pass")
findings = credential_findings(run)
if findings:
    raise SystemExit("refusing to promote capture with credentials:\n" + "\n".join(findings))
manifest = run / "evidence-manifest.json"

required = [
    "pty.typescript",
    "pty.timing",
    "tool-calls.jsonl",
    "process-lifecycle.json",
    "stores.json",
    "viewer-revisions.jsonl",
    "browser-network.jsonl",
    "obs-websocket.jsonl",
    "obs-scene-schedule.jsonl",
    "edl.json",
    "edl.sha256",
    "audio-contract.json",
    "audio-cues.json",
    "anchors.jsonl",
    "anchors-manifest.json",
    "clock-map.json",
    "readability-preflight.json",
    "ffprobe.json",
    "verification.json",
]
evidence = {}
for relative in required:
    file = run / relative
    if not file.is_file():
        raise SystemExit(f"refusing to promote: missing {relative}")
    evidence[relative] = {
        "path": repo_relative(file, ROOT),
        "bytes": file.stat().st_size,
        "sha256": sha256(file),
    }


def inventory(root: pathlib.Path) -> list[dict]:
    return [
        {
            "path": repo_relative(file, ROOT),
            "relative_path": file.relative_to(run).as_posix(),
            "bytes": file.stat().st_size,
            "sha256": sha256(file),
        }
        for file in sorted(root.rglob("*"))
        if file.is_file()
    ]


payload = {
    "contract": "kmp.obs-promoted-capture.v1",
    "scenario_id": verification["scenario_id"],
    "run_dir": repo_relative(run, ROOT),
    "raw": {
        "path": repo_relative(raw, ROOT),
        "bytes": raw.stat().st_size,
        "sha256": sha256(raw),
    },
    "run_evidence_manifest": {
        "path": repo_relative(manifest, ROOT),
        "sha256": sha256(manifest),
    },
    "evidence": evidence,
    "obs_config": inventory(run / "obs-config"),
    "review_frames": inventory(run / "review-frames"),
    "obs_logs": [
        {
            "path": repo_relative(file, ROOT),
            "bytes": file.stat().st_size,
            "sha256": sha256(file),
        }
        for file in sorted(run.glob("obs.*.log"))
    ],
}
index.parent.mkdir(parents=True, exist_ok=True)
temporary = index.with_suffix(index.suffix + ".tmp")
temporary.write_text(json.dumps(payload, indent=2) + "\n")
temporary.replace(index)
print(f"promoted index: {index}")
