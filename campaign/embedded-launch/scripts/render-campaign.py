#!/usr/bin/env python3
"""Render KMP Embedded launch masters from verified OBS raw pictures."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import subprocess
import sys

import audio_contract


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
BRIEF = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
EDL = json.loads((CAMPAIGN / "edl.json").read_text(encoding="utf-8"))
OUTPUT = ROOT / "docs" / "assets" / "campaign" / "kmp-embedded"
PACK = CAMPAIGN / "evidence-pack"


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def render_picture(video: dict[str, object], raw_root: pathlib.Path, build: pathlib.Path) -> pathlib.Path:
    """Remux a real OBS picture; still-image or DOM-terminal assembly is forbidden."""
    source = raw_root / pathlib.Path(str(video["raw_picture"])).name
    if not source.is_file():
        raise SystemExit(f"missing OBS raw picture: {source}")
    promoted_path = PACK / "capture" / "promoted" / f"{video['id']}.json"
    if not promoted_path.is_file():
        raise SystemExit(f"missing promoted OBS index: {promoted_path}")
    promoted = json.loads(promoted_path.read_text(encoding="utf-8"))
    canonical = PACK / "capture" / "raw" / f"{video['id']}.mkv"
    if promoted.get("contract") != "kmp.obs-promoted-capture.v1":
        raise SystemExit(f"{promoted_path.name} has the wrong capture contract")
    if promoted.get("scenario_id") != video["id"]:
        raise SystemExit(f"{promoted_path.name} names another scenario")
    if source.resolve() != canonical.resolve() or pathlib.Path(promoted["raw"]["path"]).resolve() != canonical.resolve():
        raise SystemExit(f"{video['id']} picture is not the canonical promoted OBS raw")
    if sha256(source) != promoted["raw"]["sha256"]:
        raise SystemExit(f"{video['id']} OBS raw hash differs from promoted capture")
    target = build / f"{video['id']}-picture.mp4"
    run(
        [
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(source),
            "-map", "0:v:0", "-c:v", "copy", "-an", "-map_metadata", "-1",
            "-movflags", "+faststart", str(target),
        ]
    )
    inspected = run(
        ["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(target)],
        capture=True,
    )
    body = json.loads(inspected.stdout)
    stream = next(item for item in body["streams"] if item["codec_type"] == "video")
    expected = EDL["picture_contract"]["canvas"]
    observed_fps = float(stream["avg_frame_rate"].split("/")[0]) / float(stream["avg_frame_rate"].split("/")[1])
    if (stream["width"], stream["height"]) != (expected["width"], expected["height"]):
        raise SystemExit(f"{source.name} is not {expected['width']}x{expected['height']}")
    if abs(observed_fps - expected["fps"]) > 0.001:
        raise SystemExit(f"{source.name} is {observed_fps} fps, expected {expected['fps']}")
    duration = float(body["format"]["duration"])
    if abs(duration - float(video["duration_seconds"])) > 1 / expected["fps"] + 0.001:
        raise SystemExit(
            f"{source.name} duration {duration} differs from {video['duration_seconds']}"
        )
    return target


def promoted_cue_resolution(master_id: str) -> tuple[dict[str, object], dict[str, object]]:
    promoted_path = PACK / "capture" / "promoted" / f"{master_id}.json"
    promoted = json.loads(promoted_path.read_text(encoding="utf-8"))
    bindings = promoted.get("evidence", {})
    cue_binding = bindings.get("audio-cues.json")
    anchor_binding = bindings.get("anchors.jsonl")
    if not isinstance(cue_binding, dict) or not isinstance(anchor_binding, dict):
        raise SystemExit(f"{master_id} promoted capture has no resolved cues/anchors")
    cue_path = pathlib.Path(str(cue_binding.get("path", ""))).resolve()
    anchor_path = pathlib.Path(str(anchor_binding.get("path", ""))).resolve()
    for label, path, binding in (
        ("resolved cues", cue_path, cue_binding), ("audio anchors", anchor_path, anchor_binding)
    ):
        if not path.is_file() or sha256(path) != binding.get("sha256"):
            raise SystemExit(f"{master_id} {label} are missing or differ from promotion")
    resolution = json.loads(cue_path.read_text(encoding="utf-8"))
    if resolution.get("contract") != "kmp.capture.audio-cue-resolution.v1":
        raise SystemExit(f"{master_id} resolved cue contract is invalid")
    if resolution.get("audio_contract_sha256") != sha256(CAMPAIGN / "audio" / "contract.json"):
        raise SystemExit(f"{master_id} resolved cues use another audio contract")
    if resolution.get("anchors_sha256") != sha256(anchor_path):
        raise SystemExit(f"{master_id} resolved cues use another anchor set")
    return resolution, {
        "anchor_path": anchor_path,
        "anchor_binding": anchor_binding,
        "cue_path": cue_path,
        "cue_binding": cue_binding,
    }


def verify_cue_anchors(video: dict[str, object]) -> dict[str, object]:
    master_id = str(video["id"])
    resolution, anchor_record = promoted_cue_resolution(master_id)
    anchor_path = pathlib.Path(anchor_record["anchor_path"])
    cue_path = pathlib.Path(anchor_record["cue_path"])
    anchors = {}
    for line in anchor_path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            item = json.loads(line)
            anchors[str(item["anchor"])] = item

    fps = float(EDL["picture_contract"]["canvas"]["fps"])
    report: list[dict[str, object]] = []
    planned = audio_contract.CONTRACT["masters"][master_id]["cue_anchors"]
    resolved = resolution.get("cues", [])
    if [(item["cue"], item["visible_anchor"]) for item in resolved] != [
        (item["cue"], item["visible_anchor"]) for item in planned
    ]:
        raise SystemExit(f"{master_id} resolved cue sequence differs from the audio contract")
    for cue in resolved:
        anchor_name = str(cue["visible_anchor"])
        anchor = anchors.get(anchor_name)
        if anchor is None:
            raise SystemExit(f"{master_id} is missing visible cue anchor {anchor_name}")
        anchor_seconds = int(anchor["video_pts_ns"]) / 1_000_000_000
        cue_seconds = float(cue["resolved_at"])
        if cue_seconds + 0.000001 < anchor_seconds:
            raise SystemExit(
                f"{master_id} cue {cue['cue']} at {cue_seconds:.6f}s precedes "
                f"visible anchor {anchor_name} at {anchor_seconds:.6f}s"
            )
        lag_frames = (cue_seconds - anchor_seconds) * fps
        max_lag = cue.get("max_lag_frames")
        if max_lag is not None and lag_frames > float(max_lag) + 0.0001:
            raise SystemExit(
                f"{master_id} cue {cue['cue']} is {lag_frames:.3f} frames after "
                f"{anchor_name}; maximum is {max_lag}"
            )
        report.append({
            "cue": cue["cue"],
            "planned_seconds": float(cue["planned_at"]),
            "cue_seconds": cue_seconds,
            "visible_anchor": anchor_name,
            "anchor_seconds": anchor_seconds,
            "lag_frames": lag_frames,
            "anchor_source": anchor.get("source"),
            "anchor_evidence": anchor.get("evidence"),
        })
    return {
        "anchors_path": str(anchor_path),
        "anchors_sha256": sha256(anchor_path),
        "cue_resolution_path": str(cue_path),
        "cue_resolution_sha256": sha256(cue_path),
        "passed": True,
        "cues": report,
    }


def silence_filters(master_id: str) -> list[str]:
    return [
        f"volume=0:enable='between(t\\,{start}\\,{end})'"
        for start, end in audio_contract.CONTRACT["masters"][master_id]["digital_silence"]
    ]


def render_mix(
    video: dict[str, object], audio: pathlib.Path, build: pathlib.Path
) -> tuple[pathlib.Path, dict[str, object]]:
    duration = float(video["duration"])
    inputs: list[str] = []
    filters: list[str] = []
    labels: list[str] = []
    for index, (cue, start) in enumerate(video["cues"]):
        source = audio / "cues" / f"{cue}.wav"
        if not source.is_file():
            raise SystemExit(f"missing procedural cue: {source}")
        inputs.extend(["-i", str(source)])
        delay = round(float(start) * 1000)
        label = f"a{index}"
        filters.append(f"[{index}:a]adelay={delay}|{delay},apad=whole_dur={duration}[{label}]")
        labels.append(f"[{label}]")
    precontrol = str(audio_contract.CONTRACT["mix"]["precontrol"])
    filters.append(
        f"{''.join(labels)}amix=inputs={len(labels)}:duration=longest:normalize=0,"
        f"{precontrol},atrim=duration={duration},asetpts=PTS-STARTPTS[mix]"
    )
    raw = build / f"{video['id']}-precontrolled.wav"
    run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *inputs,
        "-filter_complex", ";".join(filters), "-map", "[mix]", "-ar", "48000",
        "-ac", "2", "-c:a", "pcm_s24le", str(raw),
    ])

    mix_contract = audio_contract.CONTRACT["mix"]
    target_i = mix_contract["integrated_lufs"]
    target_tp = mix_contract["normalizer_true_peak_target_dbtp"]
    target_lra = mix_contract["lra_max_lu"]
    measured = run([
        "ffmpeg", "-hide_banner", "-nostats", "-i", str(raw), "-af",
        f"loudnorm=I={target_i}:TP={target_tp}:LRA={target_lra}:print_format=json",
        "-f", "null", "-",
    ], capture=True)
    match = re.search(r"\{\s*\"input_i\".*?\}", measured.stderr, re.DOTALL)
    if not match:
        raise SystemExit(f"could not parse loudnorm first pass for {video['id']}")
    values = json.loads(match.group(0))
    second_pass = (
        f"loudnorm=I={target_i}:TP={target_tp}:LRA={target_lra}:linear=true:"
        f"measured_I={values['input_i']}:measured_TP={values['input_tp']}:"
        f"measured_LRA={values['input_lra']}:measured_thresh={values['input_thresh']}:"
        f"offset={values['target_offset']}:print_format=summary"
    )
    post_filters = [
        second_pass,
        "asetpts=PTS-STARTPTS",
        "aresample=48000",
        "asetpts=PTS-STARTPTS",
        *silence_filters(str(video["id"])),
        f"atrim=duration={duration}",
        "asetpts=PTS-STARTPTS",
    ]
    mix = build / f"{video['id']}-mix.wav"
    report = run([
        "ffmpeg", "-hide_banner", "-nostats", "-y", "-i", str(raw), "-af",
        ",".join(post_filters), "-ar", "48000", "-ac", "2",
        "-c:a", "pcm_s24le", str(mix),
    ], capture=True)
    (build / f"{video['id']}-loudnorm-pass1.json").write_text(
        json.dumps(values, indent=2) + "\n", encoding="utf-8"
    )
    (build / f"{video['id']}-loudnorm-pass2.txt").write_text(
        report.stderr, encoding="utf-8"
    )
    gate = audio_contract.assert_mix(mix, str(video["id"]))
    mix_report = {
        "master_id": video["id"],
        "precontrol_filter": precontrol,
        "precontrolled": {
            "path": str(raw),
            "artifact_file_sha256": audio_contract.file_sha256(raw),
            "canonical_pcm": audio_contract.canonical_pcm(raw),
        },
        "loudnorm": {
            "passes": 2,
            "pass_1": values,
            "pass_2_filter": second_pass,
        },
        "mix": gate,
    }
    (build / f"{video['id']}-audio-report.json").write_text(
        json.dumps(mix_report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return mix, mix_report


def mux(video: dict[str, object], picture: pathlib.Path, mix: pathlib.Path) -> pathlib.Path:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    target = OUTPUT / f"{video['id']}.mp4"
    captions = CAMPAIGN / str(video["captions"])
    run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
        "-i", str(picture), "-i", str(mix), "-i", str(captions),
        "-map", "0:v:0", "-map", "1:a:0", "-map", "2:0",
        "-c:v", "copy", "-c:a", "aac", "-profile:a", "aac_low",
        "-aac_coder", "twoloop", "-b:a", "192k", "-ar", "48000", "-ac", "2",
        "-c:s", "mov_text", "-metadata:s:s:0", "language=eng",
        "-map_metadata", "-1", "-movflags", "+faststart", str(target),
    ])
    return target


def probe(target: pathlib.Path, expected: float, master_id: str) -> dict[str, object]:
    result = run([
        "ffprobe", "-v", "error", "-show_streams", "-show_format",
        "-of", "json", str(target),
    ], capture=True)
    body = json.loads(result.stdout)
    duration = float(body["format"]["duration"])
    if abs(duration - expected) > 1 / 30 + 0.001:
        raise SystemExit(f"{target.name} duration {duration} differs from {expected}")
    codecs = {stream["codec_type"]: stream["codec_name"] for stream in body["streams"]}
    if codecs != {"video": "h264", "audio": "aac", "subtitle": "mov_text"}:
        raise SystemExit(f"{target.name} has unexpected streams: {codecs}")
    first = target.read_bytes()[:1024 * 1024]
    if first.find(b"moov") < 0 or first.find(b"mdat") < first.find(b"moov"):
        raise SystemExit(f"{target.name} is not faststart")
    audio_report = audio_contract.assert_distribution_master(target, master_id)
    return {
        "path": str(target.relative_to(ROOT)),
        "duration": duration,
        "streams": codecs,
        "sha256": sha256(target),
        "audio": audio_report,
    }


def prepare_audio(build: pathlib.Path, *, picture_locked: bool = False) -> tuple[pathlib.Path, list[dict[str, object]]]:
    audio = build / "audio"
    run(["bash", str(CAMPAIGN / "scripts" / "render-audio.sh"), str(audio)])
    reports: list[dict[str, object]] = []
    for source_video in EDL["masters"]:
        cues = source_video["audio_cues"]
        if picture_locked:
            resolution, _ = promoted_cue_resolution(str(source_video["id"]))
            cues = [[item["cue"], item["resolved_at"]] for item in resolution["cues"]]
        video = {
            **source_video,
            "duration": source_video["duration_seconds"],
            "captions": source_video["caption_file"],
            "cues": cues,
        }
        _, report = render_mix(video, audio, build)
        reports.append(report)
    evidence = {
        "schema_version": "1",
        "status": "deterministic_premix_not_picture_locked",
        "contract_sha256": sha256(CAMPAIGN / "audio" / "contract.json"),
        "palette_provenance": json.loads((audio / "provenance.json").read_text(encoding="utf-8")),
        "masters": reports,
    }
    (build / "audio-evidence.json").write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return audio, reports


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "--audio-only":
        build = pathlib.Path(sys.argv[2]).resolve()
        build.mkdir(parents=True, exist_ok=True)
        prepare_audio(build)
        print(f"audio premixes: {build}")
        return
    if len(sys.argv) != 3:
        raise SystemExit(
            "usage: render-campaign.py OBS_RAW_PICTURE_DIR BUILD_DIR\n"
            "       render-campaign.py --audio-only BUILD_DIR"
        )
    raw_root = pathlib.Path(sys.argv[1]).resolve()
    build = pathlib.Path(sys.argv[2]).resolve()
    build.mkdir(parents=True, exist_ok=True)
    audio, premix_reports = prepare_audio(build, picture_locked=True)

    manifest: dict[str, object] = {
        "schema_version": "1",
        "campaign_id": BRIEF["campaign_id"],
        "status": "candidate_unapproved",
        "publishable": False,
        "approval_manifest": "campaign/embedded-launch/evidence-pack/manifest.json",
        "audio_contract": audio_contract.CONTRACT,
        "audio_premix_reports": premix_reports,
        "masters": [],
    }
    for source_video in EDL["masters"]:
        video = {
            **source_video,
            "duration": source_video["duration_seconds"],
            "captions": source_video["caption_file"],
            "cues": source_video["audio_cues"],
        }
        picture = render_picture(video, raw_root, build)
        anchor_report = verify_cue_anchors(video)
        mix = build / f"{video['id']}-mix.wav"
        target = mux(video, picture, mix)
        master_report = probe(target, float(video["duration"]), str(video["id"]))
        master_report["cue_anchor_gate"] = anchor_report
        manifest["masters"].append(master_report)
    (OUTPUT / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
