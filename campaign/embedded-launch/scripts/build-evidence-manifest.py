#!/usr/bin/env python3
"""Build or verify the transitive KMP Embedded campaign evidence manifest."""

from __future__ import annotations

import hashlib
import json
import pathlib
import sys

import panel_contract


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
MANIFEST = PACK / "manifest.json"

REQUIRED = [
    "campaign/embedded-launch/campaign.json",
    "campaign/embedded-launch/claims.json",
    "campaign/embedded-launch/creative.md",
    "campaign/embedded-launch/edl.json",
    "campaign/embedded-launch/scenario-contracts.json",
    "campaign/embedded-launch/social.md",
    "campaign/embedded-launch/evidence-pack/critic-input.json",
    "campaign/embedded-launch/captions/fresh-process-same-why.vtt",
    "campaign/embedded-launch/captions/two-processes-one-memory.vtt",
    "campaign/embedded-launch/captions/keep-the-wrong-turn.vtt",
    "campaign/embedded-launch/evidence-pack/release/commit.txt",
    "campaign/embedded-launch/evidence-pack/release/tag.txt",
    "campaign/embedded-launch/evidence-pack/release/quality-gates.json",
    "campaign/embedded-launch/evidence-pack/release/public-paths.json",
    "campaign/embedded-launch/evidence-pack/product/binary.sha256",
    "campaign/embedded-launch/evidence-pack/product/version.txt",
    "campaign/embedded-launch/evidence-pack/product/tools-list.json",
    "campaign/embedded-launch/evidence-pack/capture/raw/fresh-process-same-why.mkv",
    "campaign/embedded-launch/evidence-pack/capture/raw/fresh-process-same-why.mkv.sha256",
    "campaign/embedded-launch/evidence-pack/capture/raw/two-processes-one-memory.mkv",
    "campaign/embedded-launch/evidence-pack/capture/raw/two-processes-one-memory.mkv.sha256",
    "campaign/embedded-launch/evidence-pack/capture/raw/keep-the-wrong-turn.mkv",
    "campaign/embedded-launch/evidence-pack/capture/raw/keep-the-wrong-turn.mkv.sha256",
    "campaign/embedded-launch/evidence-pack/capture/promoted/fresh-process-same-why.json",
    "campaign/embedded-launch/evidence-pack/capture/promoted/two-processes-one-memory.json",
    "campaign/embedded-launch/evidence-pack/capture/promoted/keep-the-wrong-turn.json",
    "campaign/embedded-launch/evidence-pack/audio/provenance.json",
    "campaign/embedded-launch/evidence-pack/audio/cues.tsv",
    "campaign/embedded-launch/evidence-pack/audio/SHA256SUMS",
    "docs/assets/campaign/kmp-embedded/manifest.json",
    "docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4",
    "docs/assets/campaign/kmp-embedded/two-processes-one-memory.mp4",
    "docs/assets/campaign/kmp-embedded/keep-the-wrong-turn.mp4",
    "campaign/embedded-launch/evidence-pack/masters/ffprobe/fresh-process-same-why.json",
    "campaign/embedded-launch/evidence-pack/masters/ffprobe/two-processes-one-memory.json",
    "campaign/embedded-launch/evidence-pack/masters/ffprobe/keep-the-wrong-turn.json",
    "campaign/embedded-launch/evidence-pack/masters/SHA256SUMS",
    "campaign/embedded-launch/evidence-pack/qa/automated.json",
    "campaign/embedded-launch/evidence-pack/qa/mobile-muted-panel.json",
    "campaign/embedded-launch/evidence-pack/qa/audio-panel.json",
    "campaign/embedded-launch/evidence-pack/reproduction/commands.txt",
    "campaign/embedded-launch/evidence-pack/reproduction/tool-versions.json",
    "campaign/embedded-launch/evidence-pack/reproduction/clean-render-comparison.json",
    "campaign/embedded-launch/evidence-pack/reproduction/build.log",
    "campaign/embedded-launch/evidence-pack/signoffs/marketing.json",
    "campaign/embedded-launch/evidence-pack/signoffs/audio.json",
]


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact(path: pathlib.Path) -> dict[str, object]:
    return {
        "path": path.relative_to(ROOT).as_posix(),
        "sha256": sha256(path),
        "bytes": path.stat().st_size,
    }


def validate_file_binding(
    binding: dict[str, object],
    *,
    label: str,
    invalid: list[str],
) -> pathlib.Path | None:
    raw_path = binding.get("path")
    expected_sha = binding.get("sha256")
    expected_bytes = binding.get("bytes")
    if not isinstance(raw_path, str) or not isinstance(expected_sha, str):
        invalid.append(f"{label} has no path/SHA-256 binding")
        return None
    path = pathlib.Path(raw_path)
    if not path.is_file():
        invalid.append(f"{label} is missing: {path}")
        return None
    if sha256(path) != expected_sha:
        invalid.append(f"{label} SHA-256 differs: {path}")
    if expected_bytes is not None and path.stat().st_size != expected_bytes:
        invalid.append(f"{label} byte count differs: {path}")
    return path


def validate_promoted_capture(master_id: str, invalid: list[str]) -> None:
    index_path = PACK / "capture" / "promoted" / f"{master_id}.json"
    if not index_path.is_file():
        return
    try:
        promoted = json.loads(index_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as error:
        invalid.append(f"{master_id} promoted index is unreadable: {error}")
        return
    if promoted.get("contract") != "kmp.obs-promoted-capture.v1":
        invalid.append(f"{master_id} promoted index has the wrong contract")
    if promoted.get("scenario_id") != master_id:
        invalid.append(f"{master_id} promoted index names another scenario")

    raw = validate_file_binding(
        promoted.get("raw", {}), label=f"{master_id} OBS raw", invalid=invalid
    )
    expected_raw = (PACK / "capture" / "raw" / f"{master_id}.mkv").resolve()
    if raw is not None and raw.resolve() != expected_raw:
        invalid.append(f"{master_id} promoted raw does not use the canonical path")

    run_dir_value = promoted.get("run_dir")
    if not isinstance(run_dir_value, str):
        invalid.append(f"{master_id} promoted index has no run_dir")
        return
    run_dir = pathlib.Path(run_dir_value).resolve()
    allowed_runs = (PACK / "capture" / "runs" / master_id).resolve()
    if run_dir.parent != allowed_runs:
        invalid.append(f"{master_id} run_dir is outside its campaign run root")
        return

    run_manifest = validate_file_binding(
        promoted.get("run_evidence_manifest", {}),
        label=f"{master_id} run evidence manifest",
        invalid=invalid,
    )
    if run_manifest is None:
        return
    if run_manifest.resolve() != run_dir / "evidence-manifest.json":
        invalid.append(f"{master_id} run manifest is outside the promoted run")
        return
    try:
        run_manifest_body = json.loads(run_manifest.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as error:
        invalid.append(f"{master_id} run manifest is unreadable: {error}")
        return
    if run_manifest_body.get("contract") != "kmp.obs-evidence-pack.v1":
        invalid.append(f"{master_id} run manifest has the wrong contract")
    if run_manifest_body.get("scenario_id") != master_id:
        invalid.append(f"{master_id} run manifest names another scenario")
    if raw is not None and run_manifest_body.get("recording", {}).get("sha256") != sha256(raw):
        invalid.append(f"{master_id} raw and run recording hashes differ")

    for item in run_manifest_body.get("files", []):
        relative = item.get("path")
        if not isinstance(relative, str):
            invalid.append(f"{master_id} run manifest contains a pathless file")
            continue
        candidate = (run_dir / relative).resolve()
        try:
            candidate.relative_to(run_dir)
        except ValueError:
            invalid.append(f"{master_id} run manifest escapes run_dir: {relative}")
            continue
        validate_file_binding(
            {**item, "path": str(candidate)},
            label=f"{master_id} run artifact {relative}",
            invalid=invalid,
        )

    evidence = promoted.get("evidence", {})
    required_evidence = {
        "pty.typescript", "pty.timing", "tool-calls.jsonl",
        "process-lifecycle.json", "stores.json", "viewer-revisions.jsonl",
        "browser-network.jsonl", "obs-websocket.jsonl",
        "obs-scene-schedule.jsonl", "edl.json", "edl.sha256",
        "audio-contract.json", "audio-cues.json", "anchors.jsonl",
        "anchors-manifest.json", "clock-map.json", "readability-preflight.json",
        "ffprobe.json", "verification.json",
    }
    if not isinstance(evidence, dict) or set(evidence) != required_evidence:
        invalid.append(f"{master_id} promoted evidence inventory is incomplete")
    else:
        for relative, binding in evidence.items():
            path = validate_file_binding(
                binding,
                label=f"{master_id} promoted evidence {relative}",
                invalid=invalid,
            )
            if path is not None and path.resolve() != run_dir / relative:
                invalid.append(f"{master_id} promoted evidence points outside its run: {relative}")
        verification_path = run_dir / "verification.json"
        if verification_path.is_file():
            verification = json.loads(verification_path.read_text(encoding="utf-8"))
            if verification.get("passed") is not True:
                invalid.append(f"{master_id} promoted capture did not pass verification")
        captured_edl = run_dir / "edl.json"
        captured_edl_sha = run_dir / "edl.sha256"
        if captured_edl.is_file() and sha256(captured_edl) != sha256(CAMPAIGN / "edl.json"):
            invalid.append(f"{master_id} captured a stale EDL")
        if captured_edl.is_file() and captured_edl_sha.is_file():
            if captured_edl_sha.read_text(encoding="utf-8").split()[0] != sha256(captured_edl):
                invalid.append(f"{master_id} captured EDL hash file is stale")


def referenced_files() -> list[pathlib.Path]:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    fixed = [
        CAMPAIGN / "README.md",
        CAMPAIGN / "campaign.json",
        CAMPAIGN / "claims.json",
        CAMPAIGN / "creative.md",
        CAMPAIGN / "edl.json",
        CAMPAIGN / "scenario-contracts.json",
        CAMPAIGN / "social.md",
        CAMPAIGN / "scripts" / "derive-readme-gif.sh",
        CAMPAIGN / "scripts" / "render-campaign.py",
        CAMPAIGN / "scripts" / "validate-campaign.py",
        CAMPAIGN / "scripts" / "build-evidence-manifest.py",
        CAMPAIGN / "scripts" / "build-critic-input.py",
        CAMPAIGN / "scripts" / "build-publication-manifest.py",
        CAMPAIGN / "scripts" / "prepare-mobile-muted-panel.py",
        ROOT / "scripts" / "demo" / "record-chronoloom-gifs.js",
        ROOT / "scripts" / "demo" / "record-chronoloom-gifs.sh",
        ROOT / "README.md",
    ]
    campaign_binary = ROOT / brief["binary"]["path"]
    if campaign_binary.is_file():
        fixed.append(campaign_binary)
    for source_root in [
        CAMPAIGN / "audio",
        CAMPAIGN / "obs-harness",
        CAMPAIGN / "scripts",
        CAMPAIGN / "schema",
    ]:
        fixed.extend(
            path
            for path in sorted(source_root.rglob("*"))
            if path.is_file() and "__pycache__" not in path.parts and path.suffix != ".pyc"
        )
    fixed.extend(sorted((CAMPAIGN / "captions").glob("*.vtt")))
    fixed.extend(
        path
        for path in sorted(PACK.rglob("*"))
        if path.is_file()
        and path != MANIFEST
        and not path.is_relative_to(PACK / "capture" / "runs")
        and path not in {
            PACK / "signoffs" / "launch-critic.json",
            PACK / "publication-manifest.json",
        }
    )
    for index in sorted((PACK / "capture" / "promoted").glob("*.json")):
        try:
            promoted = json.loads(index.read_text(encoding="utf-8"))
            run_dir = pathlib.Path(promoted["run_dir"]).resolve()
            run_dir.relative_to((PACK / "capture" / "runs").resolve())
        except (json.JSONDecodeError, KeyError, OSError, ValueError):
            continue
        fixed.extend(path for path in sorted(run_dir.rglob("*")) if path.is_file())
    fixed.extend(
        path
        for path in [
            ROOT / "docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4",
            ROOT / "docs/assets/campaign/kmp-embedded/two-processes-one-memory.mp4",
            ROOT / "docs/assets/campaign/kmp-embedded/keep-the-wrong-turn.mp4",
            ROOT / "docs/assets/campaign/kmp-embedded/manifest.json",
        ]
        if path.is_file()
    )
    return sorted(set(fixed))


def build() -> dict[str, object]:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    claims = json.loads((CAMPAIGN / "claims.json").read_text(encoding="utf-8"))
    edl = json.loads((CAMPAIGN / "edl.json").read_text(encoding="utf-8"))
    missing = [item for item in REQUIRED if not (ROOT / item).is_file()]
    invalid: list[str] = []

    binary = ROOT / brief["binary"]["path"]
    if not binary.is_file():
        invalid.append(f"campaign binary does not exist: {brief['binary']['path']}")
    elif sha256(binary) != brief["binary"]["sha256"]:
        invalid.append("campaign binary hash differs from campaign.json")
    product_sha = PACK / "product" / "binary.sha256"
    if product_sha.is_file() and product_sha.read_text(encoding="utf-8").split()[0] != brief["binary"]["sha256"]:
        invalid.append("product/binary.sha256 differs from campaign.json")
    product_version = PACK / "product" / "version.txt"
    if product_version.is_file() and product_version.read_text(encoding="utf-8").strip() != brief["binary"]["version"]:
        invalid.append("product/version.txt differs from campaign.json")
    release_commit = PACK / "release" / "commit.txt"
    if release_commit.is_file() and release_commit.read_text(encoding="utf-8").strip() != brief["product_commit"]:
        invalid.append("release/commit.txt differs from campaign.json")

    candidate_manifest_path = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded" / "manifest.json"
    if candidate_manifest_path.is_file():
        candidate_manifest = json.loads(candidate_manifest_path.read_text(encoding="utf-8"))
        if candidate_manifest.get("status") != "candidate_unapproved":
            invalid.append("campaign master manifest is not a current OBS candidate")
        elif candidate_manifest.get("publishable") is not False:
            invalid.append("campaign master manifest is publishable before independent approval")
        else:
            master_bindings = {item.get("path"): item for item in candidate_manifest.get("masters", [])}
            for master in brief["masters"]:
                relative = f"docs/assets/campaign/kmp-embedded/{master['id']}.mp4"
                path = ROOT / relative
                binding = master_bindings.get(relative)
                if binding is None:
                    invalid.append(f"campaign master manifest does not bind {master['id']}")
                elif path.is_file() and binding.get("sha256") != sha256(path):
                    invalid.append(f"campaign master manifest hash differs for {master['id']}")

    derivative = edl["readme_derivative"]
    if derivative["source_master_id"] != "fresh-process-same-why":
        invalid.append("README GIF source is not campaign master 1")
    if derivative["other_gif_derivatives_allowed"] is not False:
        invalid.append("EDL permits more than one GIF derivative")
    for master in brief["masters"]:
        validate_promoted_capture(master["id"], invalid)

    master_hashes = {
        str(master["id"]): sha256(
            ROOT / "docs/assets/campaign/kmp-embedded" / f"{master['id']}.mp4"
        )
        for master in brief["masters"]
        if (ROOT / "docs/assets/campaign/kmp-embedded" / f"{master['id']}.mp4").is_file()
    }
    mobile_panel = PACK / "qa/mobile-muted-panel.json"
    audio_panel = PACK / "qa/audio-panel.json"
    if mobile_panel.is_file() and len(master_hashes) == len(brief["masters"]):
        invalid.extend(
            panel_contract.validate_mobile(
                mobile_panel,
                campaign_id=str(brief["campaign_id"]),
                master_hashes=master_hashes,
                material=PACK / "qa/mobile-muted-material/material-manifest.json",
            )
        )
    if audio_panel.is_file() and len(master_hashes) == len(brief["masters"]):
        invalid.extend(
            panel_contract.validate_audio(
                audio_panel,
                campaign_id=str(brief["campaign_id"]),
                master_hashes=master_hashes,
            )
        )

    return {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "status": "complete" if not missing and not invalid else "incomplete",
        "product": {
            "commit": brief["product_commit"],
            "binary": brief["binary"],
        },
        "sources": {
            "brief_sha256": sha256(CAMPAIGN / "campaign.json"),
            "claims_sha256": sha256(CAMPAIGN / "claims.json"),
            "edl_sha256": sha256(CAMPAIGN / "edl.json"),
        },
        "claim_bindings": [
            {
                "id": claim["id"],
                "master_id": claim["master_id"],
                "evidence": claim["evidence"],
            }
            for claim in claims["claims"]
        ],
        "readme_derivative": derivative,
        "artifacts": [artifact(path) for path in referenced_files()],
        "missing_required": missing,
        "invalid": invalid,
        "human_panels": {
            "mobile_muted_required": 5,
            "mobile_muted_run": mobile_panel.is_file(),
            "audio_panel_required": True,
            "audio_panel_run": audio_panel.is_file(),
        },
    }


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in {"build", "check"}:
        raise SystemExit("usage: build-evidence-manifest.py build|check")
    expected = build()
    if sys.argv[1] == "build":
        MANIFEST.parent.mkdir(parents=True, exist_ok=True)
        MANIFEST.write_text(json.dumps(expected, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"evidence manifest: {expected['status']}")
        return
    if not MANIFEST.is_file():
        raise SystemExit("evidence manifest is missing; run build first")
    actual = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if actual != expected:
        raise SystemExit("evidence manifest is stale; run build again")
    if expected["status"] != "complete":
        for item in expected["missing_required"]:
            print(f"missing: {item}", file=sys.stderr)
        for item in expected["invalid"]:
            print(f"invalid: {item}", file=sys.stderr)
        raise SystemExit("campaign evidence is incomplete")
    print("campaign evidence: complete")


if __name__ == "__main__":
    main()
