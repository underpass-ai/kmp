#!/usr/bin/env python3
"""Prepare, but never score, the 390 px muted independent-review package."""

from __future__ import annotations

import hashlib
import json
import pathlib
import subprocess


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
OUTPUT = CAMPAIGN / "evidence-pack" / "qa" / "mobile-muted-material"


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)


def main() -> None:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    OUTPUT.mkdir(parents=True, exist_ok=True)
    artifacts: list[dict[str, object]] = []
    for master in brief["masters"]:
        source = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded" / f"{master['id']}.mp4"
        if not source.is_file():
            raise SystemExit(f"mobile panel blocked: missing {source.relative_to(ROOT)}")
        muted = OUTPUT / f"{master['id']}-390px-muted.mp4"
        run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(source),
            "-map", "0:v:0", "-vf", "scale=390:-2:flags=lanczos", "-an",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18",
            "-movflags", "+faststart", str(muted),
        ])
        artifacts.append({
            "path": muted.relative_to(ROOT).as_posix(),
            "sha256": sha256(muted),
            "bytes": muted.stat().st_size,
            "role": "390px autoplay-muted review master",
        })
        frames = OUTPUT / master["id"]
        frames.mkdir(parents=True, exist_ok=True)
        for index, beat in enumerate(master["beats"], start=1):
            timestamp = (float(beat["start"]) + float(beat["end"])) / 2
            frame = frames / f"beat-{index:02d}.png"
            run([
                "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
                "-ss", f"{timestamp:.3f}", "-i", str(source), "-frames:v", "1",
                "-vf", "scale=390:-2:flags=lanczos", str(frame),
            ])
            artifacts.append({
                "path": frame.relative_to(ROOT).as_posix(),
                "sha256": sha256(frame),
                "bytes": frame.stat().st_size,
                "role": f"{master['id']} narrative beat {index} midpoint",
                "timestamp_seconds": timestamp,
            })
    manifest = {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "distribution_profile": "390px wide, autoplay muted",
        "panel_status": "not_run",
        "required_reviewers": 5,
        "reviewer_prompt": [
            "In one sentence, what product claim did this video make?",
            "What exact action did the final CTA ask you to take?",
            "Which on-screen evidence supports the claim?",
            "Name any hook, prompt, proof or CTA text you could not read naturally.",
        ],
        "artifacts": artifacts,
        "result_path": "campaign/embedded-launch/evidence-pack/qa/mobile-muted-panel.json",
    }
    target = OUTPUT / "material-manifest.json"
    target.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    response_template = {
        "schema_version": "1",
        "campaign_id": brief["campaign_id"],
        "distribution_profile": "390px wide, autoplay muted",
        "result": "PENDING",
        "material_manifest": {
            "path": target.relative_to(ROOT).as_posix(),
            "sha256": sha256(target),
        },
        "instructions": [
            "Collect responses from exactly five distinct independent humans.",
            "Each human must watch every supplied 390px muted master without coaching.",
            "Copy their answers faithfully; do not infer, complete or improve an answer.",
            "Never replace a human response with agent, OCR or automated-analysis output.",
            "Set result to PASS only when all five humans pass every required field.",
        ],
        "master_bindings": [
            {
                "id": master["id"],
                "master_sha256": sha256(
                    ROOT
                    / "docs"
                    / "assets"
                    / "campaign"
                    / "kmp-embedded"
                    / f"{master['id']}.mp4"
                ),
            }
            for master in brief["masters"]
        ],
        "reviewers": [],
    }
    template = OUTPUT / "human-response-template.json"
    template.write_text(
        json.dumps(response_template, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"mobile muted material: {target.relative_to(ROOT)} (panel not run)")
    print(f"human response template: {template.relative_to(ROOT)} (PENDING; humans only)")


if __name__ == "__main__":
    main()
