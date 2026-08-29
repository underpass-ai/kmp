#!/usr/bin/env python3
"""Adversarial tests for the human-only campaign panel contract."""

from __future__ import annotations

import copy
import hashlib
import json
import pathlib
import tempfile
import unittest

import panel_contract


CAMPAIGN_ID = "kmp-embedded-first-campaign"
MASTER_HASHES = {
    "fresh-process-same-why": "1" * 64,
    "two-processes-one-memory": "2" * 64,
    "keep-the-wrong-turn": "3" * 64,
}


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def master_reviews() -> list[dict[str, object]]:
    return [
        {
            "id": master_id,
            "master_sha256": digest,
            "claim_answer": f"claim for {master_id}",
            "cta_answer": f"CTA for {master_id}",
            "evidence_answer": f"evidence for {master_id}",
            "claim_identified": True,
            "cta_identified": True,
            "evidence_identified": True,
            "naturally_readable": True,
            "unreadable_text": [],
        }
        for master_id, digest in MASTER_HASHES.items()
    ]


class PanelContractTest(unittest.TestCase):
    def setUp(self) -> None:
        scratch_root = panel_contract.ROOT / "tmp"
        scratch_root.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(dir=scratch_root)
        self.root = pathlib.Path(self.scratch.name)
        self.material = self.root / "material-manifest.json"
        self.material.write_text('{"panel_status":"not_run"}\n', encoding="utf-8")
        self.mobile_path = self.root / "mobile.json"
        self.audio_path = self.root / "audio.json"
        self.mobile = {
            "schema_version": "1",
            "campaign_id": CAMPAIGN_ID,
            "distribution_profile": "390px wide, autoplay muted",
            "result": "PASS",
            "material_manifest": {
                "path": self.material.relative_to(panel_contract.ROOT).as_posix(),
                "sha256": sha256(self.material),
            },
            "reviewers": [
                {
                    "reviewer_id": f"human-{index}",
                    "reviewer_kind": "human",
                    "independent": True,
                    "masters": master_reviews(),
                }
                for index in range(1, 6)
            ],
        }
        self.audio = {
            "schema_version": "1",
            "campaign_id": CAMPAIGN_ID,
            "result": "PASS",
            "material_manifest": {
                "path": self.material.relative_to(panel_contract.ROOT).as_posix(),
                "sha256": sha256(self.material),
            },
            "reviewer": {
                "reviewer_id": "audio-human-1",
                "reviewer_kind": "human",
                "independent": True,
                "trained_audio_reviewer": True,
                "qualification_context": "five years of mastering work",
            },
            "masters": [
                {
                    "id": master_id,
                    "master_sha256": digest,
                    "mastering_approved": True,
                    "semantic_cues_follow_picture": True,
                    "mobile_translation_passed": True,
                    "no_false_product_sound": True,
                    "notes": f"reviewed {master_id}",
                }
                for master_id, digest in MASTER_HASHES.items()
            ],
        }

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def write(self, path: pathlib.Path, body: dict[str, object]) -> None:
        path.write_text(json.dumps(body), encoding="utf-8")

    def mobile_errors(self, body: dict[str, object]) -> list[str]:
        self.write(self.mobile_path, body)
        return panel_contract.validate_mobile(
            self.mobile_path,
            campaign_id=CAMPAIGN_ID,
            master_hashes=MASTER_HASHES,
            material=self.material,
        )

    def audio_errors(self, body: dict[str, object]) -> list[str]:
        self.write(self.audio_path, body)
        return panel_contract.validate_audio(
            self.audio_path,
            campaign_id=CAMPAIGN_ID,
            master_hashes=MASTER_HASHES,
            material=self.material,
        )

    def test_valid_human_panels_pass(self) -> None:
        self.assertEqual(self.mobile_errors(self.mobile), [])
        self.assertEqual(self.audio_errors(self.audio), [])

    def test_duplicate_or_agent_mobile_reviewer_fails(self) -> None:
        duplicate = copy.deepcopy(self.mobile)
        duplicate["reviewers"][4]["reviewer_id"] = "human-1"
        self.assertTrue(self.mobile_errors(duplicate))
        agent = copy.deepcopy(self.mobile)
        agent["reviewers"][0]["reviewer_kind"] = "agent"
        self.assertTrue(self.mobile_errors(agent))

    def test_stale_master_or_empty_answer_fails(self) -> None:
        stale = copy.deepcopy(self.mobile)
        stale["reviewers"][0]["masters"][0]["master_sha256"] = "0" * 64
        self.assertTrue(self.mobile_errors(stale))
        empty = copy.deepcopy(self.mobile)
        empty["reviewers"][0]["masters"][0]["claim_answer"] = ""
        self.assertTrue(self.mobile_errors(empty))

    def test_untrained_or_early_audio_cue_fails(self) -> None:
        untrained = copy.deepcopy(self.audio)
        untrained["reviewer"]["trained_audio_reviewer"] = False
        self.assertTrue(self.audio_errors(untrained))
        early = copy.deepcopy(self.audio)
        early["masters"][0]["semantic_cues_follow_picture"] = False
        self.assertTrue(self.audio_errors(early))

    def test_audio_panel_with_stale_or_wrong_material_fails(self) -> None:
        stale = copy.deepcopy(self.audio)
        stale["material_manifest"]["sha256"] = "0" * 64
        self.assertTrue(self.audio_errors(stale))
        wrong = copy.deepcopy(self.audio)
        wrong["material_manifest"]["path"] = "tmp/not-the-review-material.json"
        self.assertTrue(self.audio_errors(wrong))


if __name__ == "__main__":
    unittest.main()
