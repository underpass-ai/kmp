#!/usr/bin/env python3
"""Adversarial unit tests for the final-media verifier without campaign masters."""

from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).with_name("verify-final-media.py")
SPEC = importlib.util.spec_from_file_location("verify_final_media", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MEDIA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MEDIA)


def probe() -> dict[str, object]:
    return {
        "format": {"duration": "44.000000"},
        "streams": [
            {
                "codec_type": "video", "codec_name": "h264", "width": 1920,
                "height": 1080, "pix_fmt": "yuv420p", "avg_frame_rate": "30/1",
            },
            {
                "codec_type": "audio", "codec_name": "aac", "profile": "LC",
                "sample_rate": "48000", "channels": 2,
            },
            {
                "codec_type": "subtitle", "codec_name": "mov_text",
                "tags": {"language": "eng"},
            },
        ],
    }


def atom(kind: bytes, payload: bytes = b"") -> bytes:
    return (8 + len(payload)).to_bytes(4, "big") + kind + payload


class FinalMediaContractTest(unittest.TestCase):
    def test_distribution_probe_accepts_only_exact_stream_contract(self) -> None:
        MEDIA.validate_probe_body(
            probe(), expected_duration=44.0, width=1920, height=1080, fps=30.0
        )
        wrong = probe()
        wrong["streams"][0]["pix_fmt"] = "yuv444p"
        with self.assertRaises(MEDIA.GateFailure):
            MEDIA.validate_probe_body(
                wrong, expected_duration=44.0, width=1920, height=1080, fps=30.0
            )
        duplicate = probe()
        duplicate["streams"].append(dict(duplicate["streams"][1]))
        with self.assertRaises(MEDIA.GateFailure):
            MEDIA.validate_probe_body(
                duplicate, expected_duration=44.0, width=1920, height=1080, fps=30.0
            )

    def test_duration_tolerance_is_one_frame(self) -> None:
        acceptable = probe()
        acceptable["format"]["duration"] = "44.033000"
        MEDIA.validate_probe_body(
            acceptable, expected_duration=44.0, width=1920, height=1080, fps=30.0
        )
        late = probe()
        late["format"]["duration"] = "44.040000"
        with self.assertRaises(MEDIA.GateFailure):
            MEDIA.validate_probe_body(
                late, expected_duration=44.0, width=1920, height=1080, fps=30.0
            )

    def test_faststart_reads_mp4_atoms_instead_of_searching_payload(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = pathlib.Path(raw)
            fast = root / "fast.mp4"
            fast.write_bytes(atom(b"ftyp") + atom(b"moov") + atom(b"mdat", b"payload"))
            MEDIA.assert_faststart(fast)
            slow = root / "slow.mp4"
            slow.write_bytes(atom(b"ftyp") + atom(b"mdat", b"fake-moov") + atom(b"moov"))
            with self.assertRaises(MEDIA.GateFailure):
                MEDIA.assert_faststart(slow)

    def test_vtt_parser_rejects_overlap_and_preserves_copy(self) -> None:
        valid = """WEBVTT

00:00:00.000 --> 00:00:01.000
One line.

00:00:01.000 --> 00:00:02.000
Two
lines.
"""
        self.assertEqual(MEDIA.parse_webvtt(valid)[1][2], "Two lines.")
        overlap = valid.replace("00:00:01.000 -->", "00:00:00.500 -->")
        with self.assertRaises(MEDIA.GateFailure):
            MEDIA.parse_webvtt(overlap)

    def test_checksum_binding_rejects_ambiguity(self) -> None:
        digest = "a" * 64
        self.assertEqual(MEDIA.checksum_for({"fresh.mp4": digest}, "fresh"), digest)
        self.assertIsNone(
            MEDIA.checksum_for(
                {"fresh.mp4": digest, "nested/fresh.mp4": digest}, "fresh"
            )
        )

    def test_ffmpeg_round_trip_preserves_distribution_and_captions(self) -> None:
        scratch_root = MEDIA.ROOT / "tmp"
        scratch_root.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(
            prefix="campaign-final-media.", dir=scratch_root
        ) as raw:
            root = pathlib.Path(raw)
            captions = root / "captions.vtt"
            captions.write_text(
                "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nMemory, with receipts.\n",
                encoding="utf-8",
            )
            master = root / "master.mp4"
            subprocess.run(
                [
                    "ffmpeg", "-nostdin", "-v", "error", "-y",
                    "-f", "lavfi", "-i", "color=c=black:s=1920x1080:r=30:d=1",
                    "-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo:d=1",
                    "-i", str(captions), "-map", "0:v:0", "-map", "1:a:0",
                    "-map", "2:0", "-t", "1", "-c:v", "libx264", "-preset",
                    "ultrafast", "-pix_fmt", "yuv420p", "-r", "30", "-c:a",
                    "aac", "-profile:a", "aac_low", "-ar", "48000", "-ac", "2",
                    "-c:s", "mov_text", "-metadata:s:s:0", "language=eng",
                    "-movflags", "+faststart", str(master),
                ],
                check=True,
            )
            MEDIA.validate_probe_body(
                MEDIA.ffprobe(master),
                expected_duration=1.0,
                width=1920,
                height=1080,
                fps=30.0,
            )
            MEDIA.assert_faststart(master)
            MEDIA.assert_captions(master, captions, 1.0)


if __name__ == "__main__":
    unittest.main()
