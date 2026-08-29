#!/usr/bin/env python3
"""Extract auditable full/mobile scene frames and a conservative readability preflight."""

from __future__ import annotations

import hashlib
import json
import pathlib
import subprocess
import sys

from PIL import Image


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bright_line_bands(path: pathlib.Path) -> list[int]:
    image = Image.open(path).convert("RGB")
    # The rightmost 27% may be the live secondary inset. Measurements here are
    # only a line-band preflight, never a substitute for cap-height OCR/panel.
    width = max(1, round(image.width * 0.73))
    active_rows: list[int] = []
    for y in range(image.height):
        bright = 0
        for red, green, blue in (image.getpixel((x, y)) for x in range(width)):
            if max(red, green, blue) >= 145 and (red + green + blue) >= 360:
                bright += 1
        if bright >= 4:
            active_rows.append(y)
    bands: list[list[int]] = []
    for row in active_rows:
        if not bands or row - bands[-1][-1] > 2:
            bands.append([row])
        else:
            bands[-1].append(row)
    return [band[-1] - band[0] + 1 for band in bands if 3 <= band[-1] - band[0] + 1 <= 40]


if len(sys.argv) != 4:
    raise SystemExit("usage: prepare-review.py RUN_DIR SCENARIO EDL")
run = pathlib.Path(sys.argv[1]).resolve()
scenario = json.loads(pathlib.Path(sys.argv[2]).read_text())
edl = json.loads(pathlib.Path(sys.argv[3]).read_text())
master = next((item for item in edl.get("masters", []) if item["id"] == scenario["id"]), None)
schedule = (master or {}).get("obs_schedule") or [{"at_ms": 0, "scene": "KMP/Wide"}]
scene_contracts = edl.get("picture_contract", {}).get("scene_contracts", {})
thresholds = edl.get("picture_contract", {}).get("readability_acceptance", {}).get("minimum_cap_height_px", {})

full_dir = run / "review-frames" / "full-1920x1080"
mobile_dir = run / "review-frames" / "mobile-390"
full_dir.mkdir(parents=True, exist_ok=True)
mobile_dir.mkdir(parents=True, exist_ok=True)
recording = run / "obs-recording.mkv"
frames = []
for index, event in enumerate(schedule):
    next_ms = schedule[index + 1]["at_ms"] if index + 1 < len(schedule) else scenario["duration_ms"]
    span = max(1, next_ms - event["at_ms"])
    # Focus scenes may switch immediately before a long-poll revision and the
    # real DOM scroll that follows it. Sample deep enough into the same scene
    # to review settled product pixels, never a reconstructed state.
    sample_ms = event["at_ms"] + min(2000, max(250, span // 2))
    slug = event["scene"].split("/", 1)[-1].lower()
    name = f"{index + 1:02d}-{slug}-{sample_ms:06d}ms.png"
    full = full_dir / name
    mobile = mobile_dir / name
    subprocess.run(
        ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-ss", f"{sample_ms / 1000:.3f}", "-i", str(recording), "-frames:v", "1", str(full)],
        check=True,
    )
    subprocess.run(
        ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", str(full), "-vf", "scale=390:220:flags=lanczos", "-frames:v", "1", str(mobile)],
        check=True,
    )
    with Image.open(mobile) as image:
        mobile_size = list(image.size)
    bands = bright_line_bands(mobile)
    frames.append({
        "index": index + 1,
        "scene": event["scene"],
        "scene_contract": scene_contracts.get(event["scene"]),
        "scene_at_ms": event["at_ms"],
        "sample_at_ms": sample_ms,
        "full": {"path": str(full.relative_to(run)), "sha256": digest(full), "width": 1920, "height": 1080},
        "mobile": {"path": str(mobile.relative_to(run)), "sha256": digest(mobile), "width": mobile_size[0], "height": mobile_size[1]},
        "automated_bright_line_band_heights_px": bands,
        "automated_bright_line_band_median_px": sorted(bands)[len(bands) // 2] if bands else None,
    })

payload = {
    "contract": "kmp.capture.readability-preflight.v1",
    "scenario_id": scenario["id"],
    "distribution_width_px": 390,
    "target_minimum_cap_height_px": thresholds,
    "measurement_status": "preparatory_only_not_cap_height_acceptance",
    "measurement_note": "Bright line-band heights are an automated raster diagnostic, not glyph cap-height OCR and not the required muted panel.",
    "muted_panel_status": "pending_5_of_5",
    "frames": frames,
}
(run / "readability-preflight.json").write_text(json.dumps(payload, indent=2) + "\n")
print(f"review frames: {len(frames)} scenes at 1920 and 390 px")
