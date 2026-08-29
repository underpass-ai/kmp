#!/usr/bin/env python3
"""Prepare, but never score, the independent trained-human audio review."""

from __future__ import annotations

import hashlib
import json
import pathlib


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
OUTPUT = CAMPAIGN / "evidence-pack" / "qa" / "audio-panel-material"


class PreparationError(RuntimeError):
    """The canonical review material is missing or stale."""


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_object(path: pathlib.Path, label: str) -> dict[str, object]:
    if not path.is_file():
        raise PreparationError(f"missing {label}: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PreparationError(f"unreadable {label}: {path}: {error}") from error
    if not isinstance(value, dict):
        raise PreparationError(f"{label} is not a JSON object: {path}")
    return value


def write_json(path: pathlib.Path, value: object) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def prepare(
    *, campaign: pathlib.Path = CAMPAIGN, root: pathlib.Path = ROOT,
    output: pathlib.Path = OUTPUT,
) -> tuple[pathlib.Path, pathlib.Path]:
    brief = read_object(campaign / "campaign.json", "campaign brief")
    masters_root = root / "docs" / "assets" / "campaign" / "kmp-embedded"
    candidate = read_object(masters_root / "manifest.json", "candidate master manifest")
    if candidate.get("status") != "candidate_unapproved" or candidate.get("publishable") is not False:
        raise PreparationError("candidate manifest is not the unapproved human-review candidate")
    candidate_bindings = {
        item.get("path"): item
        for item in candidate.get("masters", [])
        if isinstance(item, dict) and isinstance(item.get("path"), str)
    }

    master_bindings: list[dict[str, object]] = []
    for master in brief.get("masters", []):
        master_id = str(master["id"])
        relative = f"docs/assets/campaign/kmp-embedded/{master_id}.mp4"
        source = root / relative
        if not source.is_file():
            raise PreparationError(f"missing canonical MP4: {relative}")
        digest = sha256(source)
        candidate_binding = candidate_bindings.get(relative)
        if not isinstance(candidate_binding, dict) or candidate_binding.get("sha256") != digest:
            raise PreparationError(f"candidate manifest does not bind canonical MP4: {master_id}")
        master_bindings.append({
            "id": master_id,
            "path": relative,
            "sha256": digest,
            "bytes": source.stat().st_size,
            "duration_seconds": float(master["duration_seconds"]),
            "role": "canonical picture-locked MP4 for sync and audio review",
        })

    output.mkdir(parents=True, exist_ok=True)
    material = {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "panel_type": "independent trained-human audio review",
        "panel_status": "not_run",
        "required_reviewers": 1,
        "reviewer_requirements": {
            "reviewer_kind": "human",
            "independent": True,
            "trained_audio_reviewer": True,
            "qualification_context_required": True,
        },
        "playback_protocol": [
            "Review every canonical MP4 from beginning to end with its picture visible; sync is part of the review.",
            "First pass: use reliable headphones in a quiet room at a comfortable, fixed listening level.",
            "Second pass: use a phone speaker or comparable small mono speaker at an ordinary listening level.",
            "Do not extract, remaster, loudness-normalize, enhance or replace the MP4 audio.",
            "Judge the complete master, including deliberate silence and the final silent reading hold.",
        ],
        "required_master_fields": [
            "mastering_approved",
            "semantic_cues_follow_picture",
            "mobile_translation_passed",
            "no_false_product_sound",
            "notes",
        ],
        "master_bindings": master_bindings,
        "result_path": "campaign/embedded-launch/evidence-pack/qa/audio-panel.json",
    }
    material_path = output / "material-manifest.json"
    write_json(material_path, material)

    template = {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "result": "PENDING",
        "material_manifest": {
            "path": material_path.relative_to(root).as_posix(),
            "sha256": sha256(material_path),
        },
        "instructions": [
            "This file is a response template, not a review or approval.",
            "One independent trained human must perform both playback passes and fill every field from their own observation.",
            "Record qualification_context in the reviewer's own terms; do not infer credentials.",
            "Copy notes faithfully. Agents and automated measurements cannot answer for the reviewer.",
            "Set result to PASS only after every master field is explicitly approved by that human.",
        ],
        "reviewer": {
            "reviewer_id": "",
            "reviewer_kind": "PENDING",
            "independent": None,
            "trained_audio_reviewer": None,
            "qualification_context": "",
        },
        "masters": [
            {
                "id": item["id"],
                "master_sha256": item["sha256"],
                "mastering_approved": None,
                "semantic_cues_follow_picture": None,
                "mobile_translation_passed": None,
                "no_false_product_sound": None,
                "notes": "",
            }
            for item in master_bindings
        ],
    }
    template_path = output / "human-response-template.json"
    write_json(template_path, template)
    return material_path, template_path


def main() -> None:
    try:
        material, template = prepare()
    except PreparationError as error:
        raise SystemExit(f"audio panel blocked: {error}") from error
    print(f"audio panel material: {material.relative_to(ROOT)} (panel not run)")
    print(f"human response template: {template.relative_to(ROOT)} (PENDING; human only)")


if __name__ == "__main__":
    main()
