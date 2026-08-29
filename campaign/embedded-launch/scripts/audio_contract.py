#!/usr/bin/env python3
"""Fail-closed audio measurements for the KMP Embedded launch campaign."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import subprocess
import sys
from typing import Any


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
CONTRACT_PATH = CAMPAIGN / "audio" / "contract.json"
CONTRACT = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))


def run(command: list[str], *, binary: bool = False) -> subprocess.CompletedProcess[Any]:
    return subprocess.run(
        command,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=not binary,
    )


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_pcm(path: pathlib.Path) -> dict[str, Any]:
    """Hash decoded samples, never the metadata-bearing WAV container."""
    pcm = CONTRACT["pcm"]
    process = subprocess.Popen(
        [
            "ffmpeg", "-nostdin", "-v", "error", "-i", str(path),
            "-map", "0:a:0", "-ar", str(pcm["sample_rate_hz"]),
            "-ac", str(pcm["channels"]), "-f", "s24le", "-",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    digest = hashlib.sha256()
    byte_count = 0
    for chunk in iter(lambda: process.stdout.read(1024 * 1024), b""):
        digest.update(chunk)
        byte_count += len(chunk)
    stderr = process.stderr.read().decode("utf-8", errors="replace") if process.stderr else ""
    if process.wait() != 0:
        raise RuntimeError(f"could not decode canonical PCM from {path}: {stderr}")
    frame_bytes = int(pcm["channels"]) * 3
    if byte_count % frame_bytes:
        raise RuntimeError(f"decoded PCM from {path} is not whole 24-bit stereo frames")
    return {
        "format": pcm["canonical_hash_format"],
        "sha256": digest.hexdigest(),
        "bytes": byte_count,
        "frames": byte_count // frame_bytes,
    }


def probe(path: pathlib.Path) -> dict[str, Any]:
    result = run([
        "ffprobe", "-v", "error", "-show_streams", "-show_format",
        "-of", "json", str(path),
    ])
    body = json.loads(result.stdout)
    audio = next((stream for stream in body["streams"] if stream["codec_type"] == "audio"), None)
    if audio is None:
        raise RuntimeError(f"{path} has no audio stream")
    return {"stream": audio, "format": body["format"]}


def loudness(path: pathlib.Path) -> dict[str, float]:
    mix = CONTRACT["mix"]
    measured = run([
        "ffmpeg", "-hide_banner", "-nostats", "-i", str(path), "-af",
        f"loudnorm=I={mix['integrated_lufs']}:TP={mix['true_peak_max_dbtp']}:"
        f"LRA={mix['lra_max_lu']}:print_format=json",
        "-f", "null", "-",
    ])
    match = re.search(r"\{\s*\"input_i\".*?\}", measured.stderr, re.DOTALL)
    if not match:
        raise RuntimeError(f"could not parse loudness report for {path}")
    values = json.loads(match.group(0))
    return {
        "integrated_lufs": float(values["input_i"]),
        "true_peak_dbtp": float(values["input_tp"]),
        "lra_lu": float(values["input_lra"]),
        "threshold_lufs": float(values["input_thresh"]),
    }


def interval_is_zero(path: pathlib.Path, start: float, end: float) -> dict[str, Any]:
    pcm = CONTRACT["pcm"]
    result = run([
        "ffmpeg", "-nostdin", "-v", "error", "-i", str(path),
        "-map", "0:a:0", "-af", f"atrim=start={start}:end={end},asetpts=PTS-STARTPTS",
        "-ar", str(pcm["sample_rate_hz"]), "-ac", str(pcm["channels"]),
        "-f", "s24le", "-",
    ], binary=True)
    data = result.stdout
    maximum = max(
        (abs(int.from_bytes(data[offset:offset + 3], "little", signed=True))
         for offset in range(0, len(data), 3)),
        default=0,
    )
    return {
        "start": start,
        "end": end,
        "decoded_bytes": len(data),
        "digital_zero": not any(data),
        "max_abs_sample": maximum,
    }


def assert_pcm_24(path: pathlib.Path) -> dict[str, Any]:
    body = probe(path)
    stream = body["stream"]
    if stream.get("codec_name") != "pcm_s24le":
        raise RuntimeError(f"{path} is {stream.get('codec_name')}, expected pcm_s24le")
    if int(stream.get("sample_rate", 0)) != int(CONTRACT["pcm"]["sample_rate_hz"]):
        raise RuntimeError(f"{path} does not use 48 kHz PCM")
    if int(stream.get("channels", 0)) != int(CONTRACT["pcm"]["channels"]):
        raise RuntimeError(f"{path} is not stereo")
    bits = int(stream.get("bits_per_raw_sample") or stream.get("bits_per_sample") or 0)
    if bits != int(CONTRACT["pcm"]["bits_per_sample"]):
        raise RuntimeError(f"{path} has {bits} significant bits, expected 24")
    return body


def assert_mix(path: pathlib.Path, master_id: str) -> dict[str, Any]:
    assert_pcm_24(path)
    measured = loudness(path)
    mix = CONTRACT["mix"]
    failures: list[str] = []
    if abs(measured["integrated_lufs"] - float(mix["integrated_lufs"])) > float(mix["integrated_tolerance_lu"]):
        failures.append(f"integrated loudness is {measured['integrated_lufs']} LUFS")
    if measured["true_peak_dbtp"] > float(mix["true_peak_max_dbtp"]):
        failures.append(f"true peak is {measured['true_peak_dbtp']} dBTP")
    if measured["lra_lu"] > float(mix["lra_max_lu"]):
        failures.append(f"LRA is {measured['lra_lu']} LU")
    silence = [
        interval_is_zero(path, float(start), float(end))
        for start, end in CONTRACT["masters"][master_id]["digital_silence"]
    ]
    for interval in silence:
        if not interval["digital_zero"]:
            failures.append(
                f"PCM is non-zero during {interval['start']}..{interval['end']}"
            )
    report = {
        "path": str(path),
        "file_sha256": file_sha256(path),
        "canonical_pcm": canonical_pcm(path),
        "loudness": measured,
        "digital_silence": silence,
        "passed": not failures,
        "failures": failures,
    }
    if failures:
        raise RuntimeError(f"{master_id} audio gate failed: {'; '.join(failures)}")
    return report


def assert_aac(path: pathlib.Path) -> dict[str, Any]:
    body = probe(path)
    stream = body["stream"]
    distribution = CONTRACT["distribution"]
    failures: list[str] = []
    if stream.get("codec_name") != distribution["codec"]:
        failures.append(f"codec is {stream.get('codec_name')}")
    if stream.get("profile") != distribution["profile"]:
        failures.append(f"AAC profile is {stream.get('profile')}")
    if int(stream.get("sample_rate", 0)) != int(distribution["sample_rate_hz"]):
        failures.append(f"sample rate is {stream.get('sample_rate')}")
    if int(stream.get("channels", 0)) != int(distribution["channels"]):
        failures.append(f"channel count is {stream.get('channels')}")
    if failures:
        raise RuntimeError(f"{path} AAC gate failed: {'; '.join(failures)}")
    return {
        "codec": stream.get("codec_name"),
        "profile": stream.get("profile"),
        "sample_rate_hz": int(stream["sample_rate"]),
        "channels": int(stream["channels"]),
        "encoder_target_bits_per_second": distribution["encoder_target_bits_per_second"],
        "observed_average_bits_per_second": int(stream["bit_rate"]) if stream.get("bit_rate") else None,
        "bitrate_contract": distribution["bitrate_contract"],
        "decoded_pcm": canonical_pcm(path),
    }


def assert_distribution_master(path: pathlib.Path, master_id: str) -> dict[str, Any]:
    stream = assert_aac(path)
    measured = loudness(path)
    mix = CONTRACT["mix"]
    distribution = CONTRACT["distribution"]
    failures: list[str] = []
    if abs(measured["integrated_lufs"] - float(mix["integrated_lufs"])) > float(mix["integrated_tolerance_lu"]):
        failures.append(f"decoded integrated loudness is {measured['integrated_lufs']} LUFS")
    if measured["true_peak_dbtp"] > float(mix["true_peak_max_dbtp"]):
        failures.append(f"decoded true peak is {measured['true_peak_dbtp']} dBTP")
    if measured["lra_lu"] > float(mix["lra_max_lu"]):
        failures.append(f"decoded LRA is {measured['lra_lu']} LU")
    silence = [
        interval_is_zero(path, float(start), float(end))
        for start, end in CONTRACT["masters"][master_id]["digital_silence"]
    ]
    ceiling = int(distribution["decoded_silence_max_abs_sample"])
    for interval in silence:
        if int(interval["max_abs_sample"]) > ceiling:
            failures.append(
                f"decoded AAC exceeds silence ceiling during {interval['start']}..{interval['end']}: "
                f"{interval['max_abs_sample']} > {ceiling}"
            )
    report = {
        "path": str(path),
        "artifact_file_sha256": file_sha256(path),
        "stream": stream,
        "loudness": measured,
        "decoded_silence": silence,
        "passed": not failures,
        "failures": failures,
    }
    if failures:
        raise RuntimeError(f"{master_id} distribution audio gate failed: {'; '.join(failures)}")
    return report


def tool_version(command: list[str]) -> str:
    result = run(command)
    return (result.stdout or result.stderr).splitlines()[0]


def write_palette_provenance(build: pathlib.Path) -> None:
    assets = [build / "evidence-knot-palette.wav", *sorted((build / "cues").glob("*.wav"))]
    inventory = []
    for path in assets:
        assert_pcm_24(path)
        inventory.append({
            "path": path.relative_to(build).as_posix(),
            "artifact_file_sha256": file_sha256(path),
            "decoded_pcm": canonical_pcm(path),
        })
    provenance = {
        "schema_version": "1",
        "identity": CONTRACT["identity"],
        "license": "original procedural audio; distributed under the repository license",
        "third_party_audio": False,
        "determinism": {
            "authority": "decoded PCM hashes, not WAV container hashes",
            "source_sha256": file_sha256(CAMPAIGN / "audio" / "evidence-knot.csd"),
            "cue_map_sha256": file_sha256(CAMPAIGN / "audio" / "cues.tsv"),
            "contract_sha256": file_sha256(CONTRACT_PATH),
            "fixed_csound_seed": 11871,
        },
        "toolchain": {
            "csound": tool_version(["csound", "--version"]),
            "ffmpeg": tool_version(["ffmpeg", "-version"]),
        },
        "assets": inventory,
    }
    (build / "provenance.json").write_text(
        json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (build / "SHA256SUMS").write_text(
        "".join(f"{item['artifact_file_sha256']}  {item['path']}\n" for item in inventory),
        encoding="utf-8",
    )
    (build / "PCM-SHA256SUMS").write_text(
        "".join(
            f"{item['decoded_pcm']['sha256']}  {item['decoded_pcm']['format']}:{item['path']}\n"
            for item in inventory
        ),
        encoding="utf-8",
    )


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "palette-provenance":
        write_palette_provenance(pathlib.Path(sys.argv[2]).resolve())
        return
    raise SystemExit("usage: audio_contract.py palette-provenance BUILD_DIR")


if __name__ == "__main__":
    main()
