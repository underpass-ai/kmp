#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import unittest
import uuid

from capture_contract import credential_findings, repo_relative, resolve_repo_path


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
CAPTURE = CAMPAIGN / "evidence-pack" / "capture"
MASTER_IDS = (
    "fresh-process-same-why",
    "two-processes-one-memory",
    "keep-the-wrong-turn",
)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_script(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CapturePathContractTests(unittest.TestCase):
    def setUp(self) -> None:
        (ROOT / "tmp").mkdir(exist_ok=True)
        self.scratch = ROOT / "tmp" / f"capture-portability-test-{uuid.uuid4().hex}"
        self.scratch.mkdir()

    def tearDown(self) -> None:
        shutil.rmtree(self.scratch, ignore_errors=True)

    def test_repository_paths_relocate_and_reject_escape(self) -> None:
        first = self.scratch / "first"
        second = self.scratch / "second"
        relative = "campaign/embedded-launch/evidence-pack/capture/raw/demo.mkv"
        for root in (first, second):
            target = root / relative
            target.parent.mkdir(parents=True)
            target.write_bytes(b"same promoted bytes")
            self.assertEqual(resolve_repo_path(relative, root), target.resolve())
            self.assertEqual(repo_relative(target, root), relative)
        with self.assertRaises(ValueError):
            resolve_repo_path(str((first / relative).resolve()), first)
        with self.assertRaises(ValueError):
            resolve_repo_path("../escape", first)
        outside = self.scratch / "outside"
        outside.mkdir()
        (first / "link").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            resolve_repo_path("link/escape", first)

    def test_credential_audit_rejects_clear_values_without_echoing_them(self) -> None:
        run = self.scratch / "run"
        run.mkdir()
        (run / "obs-auth.json").write_text(
            json.dumps({
                "auth_required": True,
                "cleartext_retained": False,
            }),
            encoding="utf-8",
        )
        (run / "obs-websocket.jsonl").write_text(
            json.dumps({"authentication": "redacted"}) + "\n", encoding="utf-8"
        )
        self.assertEqual(credential_findings(run), [])
        secret = "b" * 64
        (run / "leak.json").write_text(
            json.dumps({"viewer": f"http://127.0.0.1:1/api/view?k={secret}"}),
            encoding="utf-8",
        )
        findings = credential_findings(run)
        self.assertTrue(findings)
        self.assertNotIn(secret, "\n".join(findings))

    @unittest.skipUnless(
        all((CAPTURE / "promoted" / f"{master_id}.json").is_file() for master_id in MASTER_IDS),
        "promoted capture pack is not present",
    )
    def test_real_pack_validates_after_repromotion_in_another_checkout_root(self) -> None:
        relocated = self.scratch / "relocated-checkout"
        shutil.copytree(CAMPAIGN, relocated / "campaign" / "embedded-launch", copy_function=os.link)
        relocated_campaign = relocated / "campaign" / "embedded-launch"
        relocated_capture = relocated_campaign / "evidence-pack" / "capture"
        promote = relocated_campaign / "obs-harness" / "scripts" / "promote-run.py"

        for master_id in MASTER_IDS:
            runs = sorted((relocated_capture / "runs" / master_id).iterdir())
            self.assertEqual(len(runs), 1)
            obs_auth = runs[0] / "obs-auth.json"
            auth = json.loads(obs_auth.read_text(encoding="utf-8"))
            auth.pop("password_sha256", None)
            auth_replacement = obs_auth.with_suffix(".json.tmp")
            auth_replacement.write_text(json.dumps(auth, indent=2) + "\n", encoding="utf-8")
            auth_replacement.replace(obs_auth)
            run_manifest = runs[0] / "evidence-manifest.json"
            manifest = json.loads(run_manifest.read_text(encoding="utf-8"))
            auth_binding = next(item for item in manifest["files"] if item["path"] == "obs-auth.json")
            auth_binding.update(bytes=obs_auth.stat().st_size, sha256=sha256(obs_auth))
            manifest_replacement = run_manifest.with_suffix(".json.tmp")
            manifest_replacement.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
            manifest_replacement.replace(run_manifest)
            subprocess.run(
                [
                    sys.executable,
                    str(promote),
                    str(runs[0]),
                    str(relocated_capture / "raw" / f"{master_id}.mkv"),
                    str(relocated_capture / "promoted" / f"{master_id}.json"),
                ],
                cwd=relocated,
                check=True,
                capture_output=True,
                text=True,
            )

        scripts = relocated_campaign / "scripts"
        sys.path.insert(0, str(scripts))
        try:
            evidence = load_script(
                f"relocated_evidence_{uuid.uuid4().hex}", scripts / "build-evidence-manifest.py"
            )
            render = load_script(
                f"relocated_render_{uuid.uuid4().hex}", scripts / "render-campaign.py"
            )
        finally:
            sys.path.pop(0)

        for master_id in MASTER_IDS:
            run_dir = next((relocated_capture / "runs" / master_id).iterdir())
            captured_binary_sha = json.loads(
                (run_dir / "process-lifecycle.json").read_text(encoding="utf-8")
            )["binary"]["sha256"]
            invalid: list[str] = []
            evidence.validate_promoted_capture(
                master_id,
                invalid,
                expected_binary_sha256=captured_binary_sha,
            )
            self.assertEqual(invalid, [])
            promoted = json.loads(
                (relocated_capture / "promoted" / f"{master_id}.json").read_text(
                    encoding="utf-8"
                )
            )
            paths = [
                promoted["run_dir"],
                promoted["raw"]["path"],
                promoted["run_evidence_manifest"]["path"],
                *(binding["path"] for binding in promoted["evidence"].values()),
                *(binding["path"] for binding in promoted["obs_config"]),
                *(binding["path"] for binding in promoted["review_frames"]),
                *(binding["path"] for binding in promoted["obs_logs"]),
            ]
            self.assertTrue(all(not pathlib.PurePosixPath(path).is_absolute() for path in paths))
            for binding in [
                promoted["raw"],
                promoted["run_evidence_manifest"],
                *promoted["evidence"].values(),
                *promoted["obs_config"],
                *promoted["review_frames"],
                *promoted["obs_logs"],
            ]:
                target = resolve_repo_path(binding["path"], relocated)
                self.assertEqual(sha256(target), binding["sha256"])

        for video in render.EDL["masters"]:
            picture = render.promoted_picture(video, relocated_capture / "raw")
            self.assertTrue(picture.is_file())
            resolution, _ = render.promoted_cue_resolution(str(video["id"]))
            self.assertEqual(resolution["video_id"], video["id"])

        self.assertFalse((relocated / "target" / "debug" / "kmp-mcp").exists())


if __name__ == "__main__":
    unittest.main()
