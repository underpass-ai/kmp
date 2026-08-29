#!/usr/bin/env python3
"""Adversarial tests for the audio-panel handoff generator."""

from __future__ import annotations

import importlib.util
import json
import pathlib
import tempfile
import unittest

import panel_contract


SCRIPT = pathlib.Path(__file__).with_name("prepare-audio-panel.py")
SPEC = importlib.util.spec_from_file_location("prepare_audio_panel", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
PANEL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PANEL)

MASTER_IDS = (
    "fresh-process-same-why",
    "two-processes-one-memory",
    "keep-the-wrong-turn",
)


class PrepareAudioPanelTest(unittest.TestCase):
    def setUp(self) -> None:
        scratch_root = panel_contract.ROOT / "tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(
            prefix="prepare-audio-panel.", dir=scratch_root
        )
        self.root = pathlib.Path(self.temp.name)
        self.campaign = self.root / "campaign" / "embedded-launch"
        self.output = self.campaign / "evidence-pack" / "qa" / "audio-panel-material"
        masters = self.root / "docs" / "assets" / "campaign" / "kmp-embedded"
        masters.mkdir(parents=True)
        brief_masters = []
        manifest_masters = []
        for index, master_id in enumerate(MASTER_IDS, 1):
            path = masters / f"{master_id}.mp4"
            path.write_bytes(f"canonical-master-{index}".encode())
            relative = f"docs/assets/campaign/kmp-embedded/{master_id}.mp4"
            digest = PANEL.sha256(path)
            brief_masters.append({"id": master_id, "duration_seconds": 40 + index})
            manifest_masters.append({"path": relative, "sha256": digest})
        self.campaign.mkdir(parents=True)
        (self.campaign / "campaign.json").write_text(
            json.dumps({"campaign_id": "campaign:test", "masters": brief_masters}),
            encoding="utf-8",
        )
        (masters / "manifest.json").write_text(
            json.dumps({
                "status": "candidate_unapproved",
                "publishable": False,
                "masters": manifest_masters,
            }),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def prepare(self) -> tuple[pathlib.Path, pathlib.Path]:
        return PANEL.prepare(campaign=self.campaign, root=self.root, output=self.output)

    def test_material_binds_only_the_three_canonical_mp4s(self) -> None:
        material_path, template_path = self.prepare()
        material = json.loads(material_path.read_text())
        template = json.loads(template_path.read_text())
        self.assertEqual([item["id"] for item in material["master_bindings"]], list(MASTER_IDS))
        self.assertFalse(any(item["path"].endswith(".wav") for item in material["master_bindings"]))
        self.assertEqual(template["result"], "PENDING")
        self.assertEqual(template["reviewer"]["reviewer_kind"], "PENDING")
        self.assertIsNone(template["reviewer"]["trained_audio_reviewer"])
        self.assertTrue(all(item["mastering_approved"] is None for item in template["masters"]))
        errors = panel_contract.validate_audio(
            template_path,
            campaign_id="campaign:test",
            master_hashes={item["id"]: item["sha256"] for item in material["master_bindings"]},
            material=material_path,
        )
        self.assertTrue(errors, "a PENDING template must never validate as a completed panel")

    def test_generation_is_deterministic(self) -> None:
        material, template = self.prepare()
        first = (material.read_bytes(), template.read_bytes())
        self.prepare()
        self.assertEqual(first, (material.read_bytes(), template.read_bytes()))

    def test_stale_candidate_hash_is_rejected(self) -> None:
        target = self.root / "docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4"
        target.write_bytes(b"changed-after-candidate")
        with self.assertRaises(PANEL.PreparationError):
            self.prepare()

    def test_missing_master_is_rejected(self) -> None:
        target = self.root / "docs/assets/campaign/kmp-embedded/keep-the-wrong-turn.mp4"
        target.unlink()
        with self.assertRaises(PANEL.PreparationError):
            self.prepare()

    def test_publishable_manifest_is_rejected(self) -> None:
        path = self.root / "docs/assets/campaign/kmp-embedded/manifest.json"
        body = json.loads(path.read_text())
        body["publishable"] = True
        path.write_text(json.dumps(body), encoding="utf-8")
        with self.assertRaises(PANEL.PreparationError):
            self.prepare()


if __name__ == "__main__":
    unittest.main()
