#!/usr/bin/env python3
"""Adversarial source tests for the clean-room final regeneration gate."""

from __future__ import annotations

import copy
import pathlib
import tempfile
import unittest

import final_regeneration_gate as GATE


def manifest() -> dict[str, object]:
    pcm = {
        "format": "s24le-48000hz-stereo-interleaved",
        "sha256": "a" * 64,
        "bytes": 600,
        "frames": 100,
    }
    return {
        "schema_version": "1",
        "campaign_id": "fixture",
        "status": "candidate_unapproved",
        "publishable": False,
        "approval_manifest": "campaign/embedded-launch/evidence-pack/manifest.json",
        "audio_contract": {"version": "fixture"},
        "audio_premix_reports": [
            {
                "master_id": "fresh",
                "precontrolled": {"path": "/tmp/a.wav", "canonical_pcm": pcm},
                "mix": {"path": "/tmp/b.wav", "canonical_pcm": pcm, "passed": True},
            }
        ],
        "masters": [
            {
                "path": "docs/assets/campaign/kmp-embedded/fresh.mp4",
                "sha256": "b" * 64,
                "duration": 44.0,
                "streams": {"video": "h264", "audio": "aac", "subtitle": "mov_text"},
                "audio": {"path": "/tmp/fresh.mp4", "stream": {"decoded_pcm": pcm}},
                "cue_anchor_gate": {"anchors_sha256": "c" * 64},
            }
        ],
    }


class FinalRegenerationGateTest(unittest.TestCase):
    def test_scratch_must_be_empty_real_and_below_repository_tmp(self) -> None:
        GATE.REPO_TMP.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=GATE.REPO_TMP) as raw:
            scratch = pathlib.Path(raw)
            self.assertEqual(GATE.validate_scratch(scratch), scratch.resolve())
            (scratch / "occupied").write_text("x", encoding="utf-8")
            with self.assertRaises(GATE.GateFailure):
                GATE.validate_scratch(scratch)
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaises(GATE.GateFailure):
                GATE.validate_scratch(pathlib.Path(raw))

    def test_scratch_symlink_escape_is_rejected(self) -> None:
        GATE.REPO_TMP.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory() as outside:
            link = GATE.REPO_TMP / "final-gate-symlink-test"
            try:
                link.symlink_to(outside, target_is_directory=True)
                with self.assertRaises(GATE.GateFailure):
                    GATE.validate_scratch(link)
            finally:
                link.unlink(missing_ok=True)
        with tempfile.TemporaryDirectory(dir=GATE.REPO_TMP) as parent:
            real = pathlib.Path(parent) / "real"
            real.mkdir()
            link = pathlib.Path(parent) / "inside-link"
            link.symlink_to(real, target_is_directory=True)
            with self.assertRaises(GATE.GateFailure):
                GATE.validate_scratch(link)

    def test_only_ephemeral_audio_paths_are_ignored(self) -> None:
        expected = manifest()
        regenerated = copy.deepcopy(expected)
        regenerated["audio_premix_reports"][0]["mix"]["path"] = "/another/build/mix.wav"
        regenerated["masters"][0]["audio"]["path"] = "/another/build/master.mp4"
        self.assertEqual(
            GATE.manifest_projection(expected), GATE.manifest_projection(regenerated)
        )
        regenerated["masters"][0]["path"] = "docs/assets/campaign/other.mp4"
        self.assertNotEqual(
            GATE.manifest_projection(expected), GATE.manifest_projection(regenerated)
        )

    def test_pcm_and_master_mutations_are_rejected(self) -> None:
        expected = GATE.manifest_projection(manifest())
        changed_pcm = manifest()
        changed_pcm["audio_premix_reports"][0]["mix"]["canonical_pcm"]["sha256"] = "d" * 64
        with self.assertRaises(GATE.GateFailure):
            GATE.assert_same(
                "portable renderer manifest", expected,
                GATE.manifest_projection(changed_pcm),
            )
        changed_master = manifest()
        changed_master["masters"][0]["sha256"] = "e" * 64
        with self.assertRaises(GATE.GateFailure):
            GATE.assert_same(
                "portable renderer manifest", expected,
                GATE.manifest_projection(changed_master),
            )

    def test_comparison_is_exact_portable_and_fail_closed(self) -> None:
        report = {
            "contract": GATE.CONTRACT,
            "campaign_id": "fixture",
            "source_bindings": {"campaign.json": {"sha256": "a" * 64, "bytes": 1}},
            "passed": True,
        }
        GATE.compare_expected_report(report, copy.deepcopy(report))
        changed = copy.deepcopy(report)
        changed["source_bindings"]["campaign.json"]["sha256"] = "b" * 64
        with self.assertRaises(GATE.GateFailure):
            GATE.compare_expected_report(report, changed)
        absolute = copy.deepcopy(report)
        absolute["source_bindings"]["campaign.json"]["path"] = "/tmp/source"
        with self.assertRaises(GATE.GateFailure):
            GATE.compare_expected_report(absolute, absolute)
        missing_pass = copy.deepcopy(report)
        missing_pass.pop("passed")
        with self.assertRaises(GATE.GateFailure):
            GATE.compare_expected_report(missing_pass, missing_pass)

    def test_protected_review_snapshot_detects_mutation(self) -> None:
        before = {"qa/mobile.json": {"sha256": "a" * 64, "bytes": 1}}
        after = copy.deepcopy(before)
        after["qa/mobile.json"]["sha256"] = "b" * 64
        with self.assertRaises(GATE.GateFailure):
            GATE.assert_same("protected human review records", before, after)

    def test_audio_evidence_requires_byte_identical_regeneration(self) -> None:
        GATE.REPO_TMP.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=GATE.REPO_TMP) as raw:
            checkout = pathlib.Path(raw) / "checkout"
            committed = checkout / "campaign/embedded-launch/evidence-pack/audio"
            source = checkout / "campaign/embedded-launch/audio"
            rendered = pathlib.Path(raw) / "build/audio"
            committed.mkdir(parents=True)
            source.mkdir(parents=True)
            rendered.mkdir(parents=True)
            contents = {
                "provenance.json": b'{"contract":"fixture"}\n',
                "cues.tsv": b"cue\tstart\tend\nsoft\t0\t1\n",
                "SHA256SUMS": b"a  cues/soft.wav\n",
            }
            for name, body in contents.items():
                (committed / name).write_bytes(body)
                (rendered / name).write_bytes(body)
            (source / "cues.tsv").write_bytes(contents["cues.tsv"])
            report = GATE.verify_audio_evidence(checkout, pathlib.Path(raw) / "build")
            self.assertEqual(set(report), set(contents))
            (rendered / "SHA256SUMS").write_text("drift\n", encoding="utf-8")
            with self.assertRaises(GATE.GateFailure):
                GATE.verify_audio_evidence(checkout, pathlib.Path(raw) / "build")

    def test_shell_entrypoint_is_executable_and_does_not_offer_write_mode(self) -> None:
        wrapper = pathlib.Path(__file__).with_name("final-regeneration-gate.sh")
        self.assertTrue(wrapper.is_file())
        self.assertTrue(wrapper.stat().st_mode & 0o111)
        body = wrapper.read_text(encoding="utf-8")
        self.assertIn("--scratch", body)
        self.assertNotIn("--write", body)


if __name__ == "__main__":
    unittest.main()
