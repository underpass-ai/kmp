#!/usr/bin/env python3
"""Adversarial self-test for the campaign's fail-closed audio gates."""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

import audio_contract


def expect_failure(label: str, action) -> None:
    try:
        action()
    except RuntimeError:
        return
    raise SystemExit(f"audio contract self-test failed: {label} was accepted")


def ffmpeg(*arguments: str) -> None:
    subprocess.run(
        ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y", *arguments],
        check=True,
    )


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_audio_contract.py AUDIO_BUILD_DIR")
    build = pathlib.Path(sys.argv[1]).resolve()
    fresh = build / "fresh-process-same-why-mix.wav"
    if not fresh.is_file():
        raise SystemExit(f"missing pre-mix: {fresh}")
    audio_contract.assert_mix(fresh, "fresh-process-same-why")

    with tempfile.TemporaryDirectory(prefix="kmp-audio-contract-") as raw_temp:
        temp = pathlib.Path(raw_temp)
        tagged = temp / "tagged.wav"
        ffmpeg("-i", str(fresh), "-map", "0:a:0", "-c:a", "pcm_s24le", "-metadata", "comment=container-drift", str(tagged))
        if audio_contract.file_sha256(fresh) == audio_contract.file_sha256(tagged):
            raise SystemExit("audio contract self-test failed: WAV metadata did not alter the container")
        if audio_contract.canonical_pcm(fresh)["sha256"] != audio_contract.canonical_pcm(tagged)["sha256"]:
            raise SystemExit("audio contract self-test failed: canonical PCM depends on WAV metadata")

        sixteen = temp / "sixteen-bit.wav"
        ffmpeg("-i", str(fresh), "-c:a", "pcm_s16le", str(sixteen))
        expect_failure("16-bit stem", lambda: audio_contract.assert_mix(sixteen, "fresh-process-same-why"))

        contaminated = temp / "contaminated.wav"
        ffmpeg(
            "-i", str(fresh), "-f", "lavfi", "-t", "0.10", "-i", "sine=frequency=440:sample_rate=48000",
            "-filter_complex", "[1:a]adelay=20000|20000,apad=whole_dur=44[tone];[0:a][tone]amix=inputs=2:duration=first:normalize=0[out]",
            "-map", "[out]", "-c:a", "pcm_s24le", str(contaminated),
        )
        expect_failure(
            "non-zero transition",
            lambda: audio_contract.assert_mix(contaminated, "fresh-process-same-why"),
        )

        original_lra = audio_contract.CONTRACT["mix"]["lra_max_lu"]
        audio_contract.CONTRACT["mix"]["lra_max_lu"] = 1.0
        try:
            expect_failure("LRA above limit", lambda: audio_contract.assert_mix(fresh, "fresh-process-same-why"))
        finally:
            audio_contract.CONTRACT["mix"]["lra_max_lu"] = original_lra

    print("audio contract self-test: passed")


if __name__ == "__main__":
    main()
