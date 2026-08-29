#!/usr/bin/env python3
"""Build the launch-critic input only from bound release and master evidence."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re

import jsonschema


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
SCHEMA = CAMPAIGN / "schema" / "launch-critic-input.schema.json"
SHARED_SCHEMA = pathlib.Path(
    "/home/gx10a/Documents/ai/kmp-campaign-agents/launch-critic/input.schema.json"
)
OUTPUT = PACK / "critic-input.json"


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def text(relative: str) -> str:
    path = PACK / relative
    if not path.is_file():
        raise SystemExit(f"critic input blocked: missing {path.relative_to(ROOT)}")
    value = path.read_text(encoding="utf-8").strip()
    if not value:
        raise SystemExit(f"critic input blocked: empty {path.relative_to(ROOT)}")
    return value


def body(relative: str) -> dict[str, object]:
    path = PACK / relative
    if not path.is_file():
        raise SystemExit(f"critic input blocked: missing {path.relative_to(ROOT)}")
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    if SHARED_SCHEMA.is_file() and sha256(SCHEMA) != sha256(SHARED_SCHEMA):
        raise SystemExit("critic input blocked: repository schema differs from role contract")

    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    edl = json.loads((CAMPAIGN / "edl.json").read_text(encoding="utf-8"))
    commit = text("release/commit.txt")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise SystemExit("critic input blocked: release/commit.txt is not a full Git commit")
    public = body("release/public-paths.json")
    binary_sha = text("product/binary.sha256").split()[0]
    if not re.fullmatch(r"[0-9a-f]{64}", binary_sha):
        raise SystemExit("critic input blocked: product/binary.sha256 is invalid")
    if commit != brief["product_commit"] or binary_sha != brief["binary"]["sha256"]:
        raise SystemExit("critic input blocked: release/product identity differs from campaign.json")

    edits = {master["id"]: master for master in edl["masters"]}
    masters = []
    for master in brief["masters"]:
        target = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded" / f"{master['id']}.mp4"
        if not target.is_file():
            raise SystemExit(f"critic input blocked: missing final master {target.relative_to(ROOT)}")
        masters.append({
            "id": master["id"],
            "path": target.relative_to(ROOT).as_posix(),
            "sha256": sha256(target),
            "claim_ids": edits[master["id"]]["claim_ids"],
            "platforms": ["x-video"],
        })

    value = {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "release": {
            "repository": public["repository"],
            "commit": commit,
            "tag": text("release/tag.txt"),
            "public_url": public["public_url"],
            "quality_gate_url": public["quality_gate_url"],
        },
        "product": {
            "binary_path": brief["binary"]["path"],
            "binary_sha256": binary_sha,
            "version": text("product/version.txt"),
            "tool_list_path": "campaign/embedded-launch/evidence-pack/product/tools-list.json",
        },
        "brief_path": "campaign/embedded-launch/campaign.json",
        "source_root": str(ROOT),
        "evidence_pack_root": str(PACK),
        "masters": masters,
        "role_contracts": {
            "marketing": "/home/gx10a/Documents/ai/kmp-campaign-agents/marketing-director/AGENT.md",
            "audio": "/home/gx10a/Documents/ai/kmp-campaign-agents/audio-director/AGENT.md",
        },
        "distribution_profiles": ["x-video-autoplay-muted-390px"],
        "required_human_panels": {"mobile_muted": 5, "audio_trained": 1},
    }
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator(schema).validate(value)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"critic input: {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
