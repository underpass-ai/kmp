#!/usr/bin/env python3
"""Validate, but never manufacture, the campaign's human review records."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
MOBILE = PACK / "qa/mobile-muted-panel.json"
AUDIO = PACK / "qa/audio-panel.json"
MATERIAL = PACK / "qa/mobile-muted-material/material-manifest.json"


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_object(path: pathlib.Path, label: str, errors: list[str]) -> dict[str, object] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"{label} is unreadable: {error}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{label} is not a JSON object")
        return None
    return value


def non_empty(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate_mobile(
    path: pathlib.Path,
    *,
    campaign_id: str,
    master_hashes: dict[str, str],
    material: pathlib.Path,
) -> list[str]:
    errors: list[str] = []
    body = load_object(path, "mobile muted panel", errors)
    if body is None:
        return errors
    if body.get("schema_version") != "1":
        errors.append("mobile muted panel has an unsupported schema_version")
    if body.get("campaign_id") != campaign_id:
        errors.append("mobile muted panel names another campaign")
    if body.get("distribution_profile") != "390px wide, autoplay muted":
        errors.append("mobile muted panel does not use the required distribution profile")
    if body.get("result") != "PASS":
        errors.append("mobile muted panel did not pass")
    binding = body.get("material_manifest")
    if not isinstance(binding, dict) or binding.get("path") != material.relative_to(ROOT).as_posix():
        errors.append("mobile muted panel does not bind the canonical review material")
    elif not material.is_file() or binding.get("sha256") != sha256(material):
        errors.append("mobile muted panel material hash is missing or stale")

    reviewers = body.get("reviewers")
    if not isinstance(reviewers, list) or len(reviewers) != 5:
        errors.append("mobile muted panel requires exactly five reviewers")
        return errors
    reviewer_ids: list[str] = []
    expected_ids = set(master_hashes)
    for index, reviewer in enumerate(reviewers, 1):
        label = f"mobile reviewer {index}"
        if not isinstance(reviewer, dict):
            errors.append(f"{label} is not an object")
            continue
        reviewer_id = reviewer.get("reviewer_id")
        if not non_empty(reviewer_id):
            errors.append(f"{label} has no pseudonymous reviewer_id")
        else:
            reviewer_ids.append(str(reviewer_id))
        if reviewer.get("reviewer_kind") != "human" or reviewer.get("independent") is not True:
            errors.append(f"{label} is not declared as an independent human")
        reviews = reviewer.get("masters")
        if not isinstance(reviews, list):
            errors.append(f"{label} has no master reviews")
            continue
        by_id = {
            review.get("id"): review
            for review in reviews
            if isinstance(review, dict) and isinstance(review.get("id"), str)
        }
        if set(by_id) != expected_ids or len(reviews) != len(expected_ids):
            errors.append(f"{label} did not review every master exactly once")
            continue
        for master_id, expected_hash in master_hashes.items():
            review = by_id[master_id]
            if review.get("master_sha256") != expected_hash:
                errors.append(f"{label} reviewed a stale {master_id}")
            for field in ("claim_answer", "cta_answer", "evidence_answer"):
                if not non_empty(review.get(field)):
                    errors.append(f"{label} left {master_id}.{field} empty")
            for field in (
                "claim_identified",
                "cta_identified",
                "evidence_identified",
                "naturally_readable",
            ):
                if review.get(field) is not True:
                    errors.append(f"{label} did not pass {master_id}.{field}")
            unreadable = review.get("unreadable_text")
            if not isinstance(unreadable, list) or unreadable:
                errors.append(f"{label} reported unreadable text in {master_id}")
    if len(set(reviewer_ids)) != 5:
        errors.append("mobile muted panel reviewer ids are not five distinct people")
    return errors


def validate_audio(
    path: pathlib.Path,
    *,
    campaign_id: str,
    master_hashes: dict[str, str],
) -> list[str]:
    errors: list[str] = []
    body = load_object(path, "audio panel", errors)
    if body is None:
        return errors
    if body.get("schema_version") != "1" or body.get("campaign_id") != campaign_id:
        errors.append("audio panel schema or campaign identity differs")
    if body.get("result") != "PASS":
        errors.append("audio panel did not pass")
    reviewer = body.get("reviewer")
    if not isinstance(reviewer, dict):
        errors.append("audio panel has no reviewer")
    else:
        if (
            reviewer.get("reviewer_kind") != "human"
            or reviewer.get("independent") is not True
            or reviewer.get("trained_audio_reviewer") is not True
        ):
            errors.append("audio panel reviewer is not an independent trained human")
        if not non_empty(reviewer.get("reviewer_id")) or not non_empty(
            reviewer.get("qualification_context")
        ):
            errors.append("audio panel reviewer identity or qualification is empty")
    reviews = body.get("masters")
    if not isinstance(reviews, list):
        errors.append("audio panel has no master reviews")
        return errors
    by_id = {
        review.get("id"): review
        for review in reviews
        if isinstance(review, dict) and isinstance(review.get("id"), str)
    }
    if set(by_id) != set(master_hashes) or len(reviews) != len(master_hashes):
        errors.append("audio panel did not review every master exactly once")
        return errors
    for master_id, expected_hash in master_hashes.items():
        review = by_id[master_id]
        if review.get("master_sha256") != expected_hash:
            errors.append(f"audio panel reviewed a stale {master_id}")
        for field in (
            "mastering_approved",
            "semantic_cues_follow_picture",
            "mobile_translation_passed",
            "no_false_product_sound",
        ):
            if review.get(field) is not True:
                errors.append(f"audio panel did not pass {master_id}.{field}")
        if not non_empty(review.get("notes")):
            errors.append(f"audio panel left {master_id}.notes empty")
    return errors


def campaign_identity() -> tuple[str, dict[str, str]]:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    master_hashes: dict[str, str] = {}
    for master in brief["masters"]:
        path = ROOT / "docs/assets/campaign/kmp-embedded" / f"{master['id']}.mp4"
        if not path.is_file():
            raise SystemExit(f"human panel contract: missing {path.relative_to(ROOT)}")
        master_hashes[str(master["id"])] = sha256(path)
    return str(brief["campaign_id"]), master_hashes


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("check",))
    parser.parse_args()
    campaign_id, master_hashes = campaign_identity()
    errors: list[str] = []
    errors.extend(
        validate_mobile(
            MOBILE,
            campaign_id=campaign_id,
            master_hashes=master_hashes,
            material=MATERIAL,
        )
    )
    errors.extend(validate_audio(AUDIO, campaign_id=campaign_id, master_hashes=master_hashes))
    if errors:
        raise SystemExit("human panel contract failed:\n" + "\n".join(f"- {item}" for item in errors))
    print("human panel contract: five muted-mobile humans and one trained audio human passed")


if __name__ == "__main__":
    main()
