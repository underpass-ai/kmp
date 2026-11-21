"""The performance scripts whose inputs left git with `artifacts/` refuse to
start without them, and say where to restore them from.

    python3 -m unittest discover -s scripts/performance/tests -t .
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PERFORMANCE = ROOT / "scripts" / "performance"


def run(script: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-I", str(PERFORMANCE / script), *arguments],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


class ExplicitInputs(unittest.TestCase):
    def test_proof_batch_run_needs_a_patch_dir_with_a_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch)
            usage = run("proof_batch_physical_run.py", str(ROOT), str(base / "out"), str(base / "s"))
            self.assertNotEqual(usage.returncode, 0)
            self.assertIn("PATCH_DIR", usage.stderr)

            empty = base / "patches"
            empty.mkdir()
            refused = run(
                "proof_batch_physical_run.py", str(ROOT), str(base / "out"), str(base / "s"), str(empty)
            )
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("manifest.json", refused.stderr)
            self.assertIn("v0.27.0", refused.stderr)
            self.assertFalse((base / "out").exists(), "nothing is created before the refusal")

    def test_sqlite_clients_checkpoint_needs_its_clients_input(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch)
            common = ["--scratch", str(base / "s"), "--output", str(base / "o.json"),
                      "--runner-binary", str(base / "runner"), "--checkpoint-only"]
            absent = run("sqlite_clients.py", *common)
            self.assertEqual(absent.returncode, 2)
            self.assertIn("--clients-input", absent.stderr)
            self.assertIn("v0.27.0", absent.stderr)

            missing = run("sqlite_clients.py", *common, "--clients-input", str(base / "none.json"))
            self.assertEqual(missing.returncode, 2)
            self.assertIn("no such file", missing.stderr)
            self.assertFalse((base / "s").exists(), "nothing is created before the refusal")


if __name__ == "__main__":
    unittest.main()
