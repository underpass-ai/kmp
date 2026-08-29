#!/usr/bin/env python3
"""Fail closed on the static distribution contract of final campaign masters."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import subprocess
from fractions import Fraction
from typing import Any

import audio_contract


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
MASTERS = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded"


class GateFailure(RuntimeError):
    """One final-media invariant was not proved."""


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(command: list[str], *, text: bool = True) -> subprocess.CompletedProcess[Any]:
    return subprocess.run(
        command,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=text,
    )


def ffprobe(path: pathlib.Path) -> dict[str, Any]:
    result = run([
        "ffprobe", "-v", "error", "-show_streams", "-show_format",
        "-of", "json", str(path),
    ])
    body = json.loads(result.stdout)
    if not isinstance(body, dict):
        raise GateFailure(f"ffprobe returned no object for {path}")
    return body


def stream_by_type(body: dict[str, Any]) -> dict[str, dict[str, Any]]:
    streams = body.get("streams")
    if not isinstance(streams, list):
        raise GateFailure("ffprobe output has no stream inventory")
    grouped: dict[str, list[dict[str, Any]]] = {}
    for stream in streams:
        if not isinstance(stream, dict) or not isinstance(stream.get("codec_type"), str):
            raise GateFailure("ffprobe output contains an invalid stream")
        grouped.setdefault(stream["codec_type"], []).append(stream)
    expected = {"video", "audio", "subtitle"}
    if set(grouped) != expected or any(len(grouped[kind]) != 1 for kind in expected):
        counts = {kind: len(items) for kind, items in grouped.items()}
        raise GateFailure(f"expected exactly video+audio+subtitle streams; got {counts}")
    return {kind: grouped[kind][0] for kind in expected}


def rate(value: object) -> float:
    try:
        return float(Fraction(str(value)))
    except (ValueError, ZeroDivisionError) as error:
        raise GateFailure(f"invalid frame rate {value!r}") from error


def probe_fingerprint(body: dict[str, Any]) -> dict[str, Any]:
    streams = stream_by_type(body)
    video = streams["video"]
    audio = streams["audio"]
    subtitle = streams["subtitle"]
    try:
        return {
            "duration": round(float(body["format"]["duration"]), 6),
            "video": {
                "codec": video.get("codec_name"),
                "width": int(video["width"]),
                "height": int(video["height"]),
                "pixel_format": video.get("pix_fmt"),
                "average_fps": round(rate(video.get("avg_frame_rate")), 6),
            },
            "audio": {
                "codec": audio.get("codec_name"),
                "profile": audio.get("profile"),
                "sample_rate": int(audio["sample_rate"]),
                "channels": int(audio["channels"]),
            },
            "subtitle": {
                "codec": subtitle.get("codec_name"),
                "language": subtitle.get("tags", {}).get("language"),
            },
        }
    except (KeyError, TypeError, ValueError) as error:
        raise GateFailure(f"ffprobe output is missing a distribution field: {error}") from error


def validate_probe_body(
    body: dict[str, Any],
    *,
    expected_duration: float,
    width: int,
    height: int,
    fps: float,
) -> dict[str, Any]:
    fingerprint = probe_fingerprint(body)
    failures: list[str] = []
    if abs(float(fingerprint["duration"]) - expected_duration) > 1 / fps + 0.001:
        failures.append(
            f"duration {fingerprint['duration']} differs from {expected_duration} by more than one frame"
        )
    video = fingerprint["video"]
    if video["codec"] != "h264":
        failures.append(f"video codec is {video['codec']}, expected h264")
    if (video["width"], video["height"]) != (width, height):
        failures.append(f"canvas is {video['width']}x{video['height']}, expected {width}x{height}")
    if video["pixel_format"] != "yuv420p":
        failures.append(f"pixel format is {video['pixel_format']}, expected yuv420p")
    if abs(float(video["average_fps"]) - fps) > 0.001:
        failures.append(f"average frame rate is {video['average_fps']}, expected {fps}")
    audio = fingerprint["audio"]
    if (
        audio["codec"] != "aac"
        or audio["profile"] != "LC"
        or audio["sample_rate"] != 48000
        or audio["channels"] != 2
    ):
        failures.append(f"audio stream differs from AAC-LC/48 kHz/stereo: {audio}")
    subtitle = fingerprint["subtitle"]
    if subtitle["codec"] != "mov_text" or subtitle["language"] != "eng":
        failures.append(f"subtitle stream differs from mov_text/eng: {subtitle}")
    if failures:
        raise GateFailure("; ".join(failures))
    return fingerprint


def top_level_atoms(path: pathlib.Path) -> list[bytes]:
    atoms: list[bytes] = []
    size = path.stat().st_size
    offset = 0
    with path.open("rb") as handle:
        while offset < size:
            handle.seek(offset)
            header = handle.read(8)
            if len(header) != 8:
                raise GateFailure(f"truncated MP4 atom at byte {offset}")
            atom_size = int.from_bytes(header[:4], "big")
            atom_type = header[4:8]
            header_size = 8
            if atom_size == 1:
                extended = handle.read(8)
                if len(extended) != 8:
                    raise GateFailure(f"truncated extended MP4 atom at byte {offset}")
                atom_size = int.from_bytes(extended, "big")
                header_size = 16
            elif atom_size == 0:
                atom_size = size - offset
            if atom_size < header_size or offset + atom_size > size:
                raise GateFailure(f"invalid MP4 atom {atom_type!r} at byte {offset}")
            atoms.append(atom_type)
            offset += atom_size
    return atoms


def assert_faststart(path: pathlib.Path) -> None:
    atoms = top_level_atoms(path)
    if b"moov" not in atoms or b"mdat" not in atoms or atoms.index(b"moov") > atoms.index(b"mdat"):
        raise GateFailure(f"{path.name} is not faststart (moov must precede mdat)")


TIMING = re.compile(
    r"^(?P<start>(?:\d{2}:)?\d{2}:\d{2}\.\d{3})\s+-->\s+"
    r"(?P<end>(?:\d{2}:)?\d{2}:\d{2}\.\d{3})(?:\s+.*)?$"
)


def timestamp(value: str) -> float:
    fields = value.split(":")
    if len(fields) == 2:
        hours = 0
        minutes, seconds = fields
    elif len(fields) == 3:
        hours, minutes, seconds = fields
    else:
        raise GateFailure(f"invalid WebVTT timestamp {value!r}")
    return int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def parse_webvtt(value: str) -> list[tuple[float, float, str]]:
    normalized = value.replace("\r\n", "\n").lstrip("\ufeff")
    lines = normalized.splitlines()
    if not lines or not lines[0].startswith("WEBVTT"):
        raise GateFailure("caption file has no WEBVTT header")
    blocks = re.split(r"\n\s*\n", "\n".join(lines[1:]).strip())
    cues: list[tuple[float, float, str]] = []
    for block in blocks:
        cue_lines = [line.strip() for line in block.splitlines() if line.strip()]
        if not cue_lines or cue_lines[0].startswith(("NOTE", "STYLE", "REGION")):
            continue
        timing_index = next((index for index, line in enumerate(cue_lines[:2]) if "-->" in line), None)
        if timing_index is None:
            raise GateFailure(f"caption block has no timing: {cue_lines[0]!r}")
        match = TIMING.fullmatch(cue_lines[timing_index])
        if match is None:
            raise GateFailure(f"invalid WebVTT timing: {cue_lines[timing_index]!r}")
        start = timestamp(match.group("start"))
        end = timestamp(match.group("end"))
        copy = " ".join(" ".join(cue_lines[timing_index + 1 :]).split())
        if not copy or end <= start:
            raise GateFailure("caption cue is empty or has a non-positive duration")
        if cues and start < cues[-1][1] - 0.001:
            raise GateFailure("caption cues overlap")
        cues.append((start, end, copy))
    if not cues:
        raise GateFailure("caption file contains no cues")
    return cues


def extracted_captions(path: pathlib.Path) -> list[tuple[float, float, str]]:
    result = run([
        "ffmpeg", "-nostdin", "-v", "error", "-i", str(path),
        "-map", "0:s:0", "-f", "webvtt", "-",
    ])
    return parse_webvtt(result.stdout)


def assert_captions(master: pathlib.Path, source: pathlib.Path, duration: float) -> None:
    expected = parse_webvtt(source.read_text(encoding="utf-8"))
    observed = extracted_captions(master)
    if len(expected) != len(observed):
        raise GateFailure(f"{master.name} embeds {len(observed)} caption cues; expected {len(expected)}")
    for index, (wanted, actual) in enumerate(zip(expected, observed), 1):
        if abs(wanted[0] - actual[0]) > 0.002 or abs(wanted[1] - actual[1]) > 0.002:
            raise GateFailure(f"{master.name} caption {index} timing differs from its VTT source")
        if wanted[2] != actual[2]:
            raise GateFailure(f"{master.name} caption {index} text differs from its VTT source")
    if abs(expected[-1][1] - duration) > 0.001:
        raise GateFailure(f"{source.name} does not caption through the contracted final frame")


def checksum_records(path: pathlib.Path) -> dict[str, str]:
    records: dict[str, str] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        fields = line.split(maxsplit=1)
        if len(fields) != 2 or not re.fullmatch(r"[0-9a-f]{64}", fields[0]):
            raise GateFailure(f"invalid checksum line {path}:{number}")
        name = fields[1].lstrip("* ")
        if name in records and records[name] != fields[0]:
            raise GateFailure(f"conflicting checksums for {name}")
        records[name] = fields[0]
    return records


def checksum_for(records: dict[str, str], master_id: str) -> str | None:
    suffix = f"/{master_id}.mp4"
    matches = [
        digest for name, digest in records.items()
        if name == f"{master_id}.mp4" or name.endswith(suffix)
    ]
    if len(matches) != 1:
        return None
    return matches[0]


def main() -> None:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    edl = json.loads((CAMPAIGN / "edl.json").read_text(encoding="utf-8"))
    canvas = edl["picture_contract"]["canvas"]
    manifest_path = MASTERS / "manifest.json"
    checksums_path = PACK / "masters" / "SHA256SUMS"
    if not manifest_path.is_file() or not checksums_path.is_file():
        raise GateFailure("final master manifest or SHA256SUMS is missing")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    bindings = {
        item.get("path"): item
        for item in manifest.get("masters", [])
        if isinstance(item, dict)
    }
    checksums = checksum_records(checksums_path)
    reports: list[dict[str, Any]] = []
    for contract in brief["masters"]:
        master_id = str(contract["id"])
        relative = f"docs/assets/campaign/kmp-embedded/{master_id}.mp4"
        master = ROOT / relative
        caption = CAMPAIGN / str(contract["captions"])
        stored_probe = PACK / "masters" / "ffprobe" / f"{master_id}.json"
        for required in (master, caption, stored_probe):
            if not required.is_file():
                raise GateFailure(f"missing final-media artifact: {required.relative_to(ROOT)}")
        actual_sha = sha256(master)
        binding = bindings.get(relative)
        if not isinstance(binding, dict) or binding.get("sha256") != actual_sha:
            raise GateFailure(f"candidate master manifest does not bind {master_id}")
        if checksum_for(checksums, master_id) != actual_sha:
            raise GateFailure(f"master SHA256SUMS does not bind {master_id} exactly once")
        live_probe = ffprobe(master)
        fingerprint = validate_probe_body(
            live_probe,
            expected_duration=float(contract["duration_seconds"]),
            width=int(canvas["width"]),
            height=int(canvas["height"]),
            fps=float(canvas["fps"]),
        )
        stored_fingerprint = probe_fingerprint(json.loads(stored_probe.read_text(encoding="utf-8")))
        if stored_fingerprint != fingerprint:
            raise GateFailure(f"stored ffprobe evidence differs from live {master_id}")
        assert_faststart(master)
        assert_captions(master, caption, float(contract["duration_seconds"]))
        audio = audio_contract.assert_distribution_master(master, master_id)
        reports.append({
            "id": master_id,
            "sha256": actual_sha,
            "probe": fingerprint,
            "faststart": True,
            "captions": "source VTT matches embedded mov_text",
            "audio": audio,
        })
    print(json.dumps({"passed": True, "masters": reports}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (GateFailure, RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"final media gate failed: {error}") from error
