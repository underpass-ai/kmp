#!/usr/bin/env python3
"""Bind an independently approved candidate campaign to its README derivative."""

from __future__ import annotations

import hashlib
import json
import pathlib
import sys

import jsonschema


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
EVIDENCE = PACK / "manifest.json"
CRITIC = PACK / "signoffs" / "launch-critic.json"
GIF = ROOT / "docs" / "assets" / "kmp-agent-loom.gif"
MASTER = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded" / "fresh-process-same-why.mp4"
OUTPUT = PACK / "publication-manifest.json"
SCHEMA = CAMPAIGN / "schema" / "launch-critic-output.schema.json"
SHARED_SCHEMA = pathlib.Path(
    "/home/gx10a/Documents/ai/kmp-campaign-agents/launch-critic/output.schema.json"
)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def binding(path: pathlib.Path) -> dict[str, object]:
    return {
        "path": path.relative_to(ROOT).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def main() -> None:
    if sys.argv[1:] not in ([], ["--preflight"]):
        raise SystemExit("usage: build-publication-manifest.py [--preflight]")
    preflight = sys.argv[1:] == ["--preflight"]
    required = [EVIDENCE, CRITIC, MASTER] if preflight else [EVIDENCE, CRITIC, GIF, MASTER]
    for path in required:
        if not path.is_file():
            raise SystemExit(f"publication blocked: missing {path.relative_to(ROOT)}")
    if SHARED_SCHEMA.is_file() and sha256(SCHEMA) != sha256(SHARED_SCHEMA):
        raise SystemExit("publication blocked: critic output schema differs from role contract")

    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    if evidence.get("status") != "complete":
        raise SystemExit("publication blocked: candidate evidence manifest is incomplete")
    critic = json.loads(CRITIC.read_text(encoding="utf-8"))
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker()).validate(critic)
    if critic["campaign_id"] != evidence["campaign_id"]:
        raise SystemExit("publication blocked: critic evaluated another campaign")
    if critic["input_manifest_sha256"] != sha256(EVIDENCE):
        raise SystemExit("publication blocked: critic manifest hash is stale")
    if critic["decision"] != "GO" or critic["critical_blockers"]:
        raise SystemExit("publication blocked: independent critic did not return clean GO")
    if critic["scores"]["campaign_total"] < 85:
        raise SystemExit("publication blocked: campaign score is below 85")
    if any(score < 8 for score in critic["scores"]["domains"].values()):
        raise SystemExit("publication blocked: a critic domain is below 8")

    source = next(
        (
            item for item in evidence["artifacts"]
            if item["path"] == "docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4"
        ),
        None,
    )
    if source is None or source["sha256"] != sha256(MASTER):
        raise SystemExit("publication blocked: campaign master 1 is not bound")
    if preflight:
        print("publication preflight: independent GO is current")
        return

    payload = {
        "schema_version": "1",
        "campaign_id": evidence["campaign_id"],
        "status": "approved",
        "candidate_evidence": binding(EVIDENCE),
        "independent_critic": binding(CRITIC),
        "source_master": binding(MASTER),
        "readme_derivative": binding(GIF),
        "readme": binding(ROOT / "README.md"),
        "derivation_script": binding(CAMPAIGN / "scripts" / "derive-readme-gif.sh"),
    }
    OUTPUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"publication manifest: {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
