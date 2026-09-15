#!/usr/bin/env python3
"""Apply the #539 observer to exact main in isolation, validate, build and capture."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


BASE = "fc352d7d37ac0480cc79120a6288bfd446fa4c86"
PATCH_NAMES = [
    "0001-experiment-observe-physical-proof-costs-per-serial-s.patch",
    "0002-test-opt-in-to-experimental-SQLite-observer-acceptan.patch",
    "0003-experiment-split-RPC-validation-and-guidance-overhea.patch",
]


def run(arguments: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    completed = subprocess.run(
        arguments,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )
    return completed.stdout


def sha(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> None:
    if len(sys.argv) not in (4, 5):
        raise SystemExit(
            "usage: proof_batch_physical_run.py REPOSITORY OUTPUT SCRATCH [SAMPLES]"
        )
    repository, output, scratch = [Path(value).resolve() for value in sys.argv[1:4]]
    samples = sys.argv[4] if len(sys.argv) == 5 else "20"
    if output.exists() or scratch.exists():
        raise SystemExit("OUTPUT and SCRATCH must not exist")
    if run(["git", "status", "--porcelain"], repository).strip():
        raise SystemExit("repository must be clean before creating the isolated worktree")
    actual_base = run(["git", "rev-parse", "origin/main"], repository).strip()
    if actual_base != BASE:
        raise SystemExit(f"origin/main {actual_base} differs from frozen base {BASE}")

    patch_dir = repository / "artifacts/proof-batch-physical-20260915/instrumentation-patches"
    manifest = json.loads((patch_dir / "manifest.json").read_text())
    expected = {item["file"]: item["sha256"] for item in manifest["patches"]}
    for name in PATCH_NAMES:
        if sha(patch_dir / name) != expected[name]:
            raise SystemExit(f"instrumentation patch hash changed: {name}")

    repository_tmp = repository / "tmp"
    repository_tmp.mkdir(exist_ok=True)
    isolated = Path(tempfile.mkdtemp(prefix="proof-539-patched-", dir=repository_tmp))
    frozen = isolated.parent / f"{isolated.name}-binary"
    test_output = ""
    try:
        isolated.rmdir()
        run(["git", "worktree", "add", "--detach", str(isolated), BASE], repository)
        for name in PATCH_NAMES:
            run(["git", "am", str(patch_dir / name)], isolated)
        if run(["git", "status", "--porcelain"], isolated).strip():
            raise AssertionError("patched validation worktree is dirty")
        enabled_env = dict(os.environ)
        enabled_env["EVAL539_PROFILE"] = "1"
        test_output = run(
            [
                "cargo", "test", "--locked", "-p", "kmp-adapter-embedded",
                "prepared_cached_vm_work_is_per_execution_and_partial_rows_are_counted",
                "--", "--ignored", "--nocapture",
            ],
            isolated,
            enabled_env,
        )
        run(["cargo", "build", "--locked", "-p", "kmp-mcp"], isolated)
        metadata = json.loads(
            run(["cargo", "metadata", "--no-deps", "--format-version", "1"], isolated)
        )
        shutil.copy2(Path(metadata["target_directory"]) / "debug/kmp-mcp", frozen)
        run(
            [
                sys.executable,
                str(repository / "scripts/performance/proof_batch_acceptance.py"),
                str(frozen), str(output), str(scratch), samples,
            ],
            repository,
        )
        run(
            [
                sys.executable,
                str(repository / "scripts/performance/proof_batch_physical_audit.py"),
                str(output), str(output / "physical-summary.json"),
            ],
            repository,
        )
        (output / "patch-validation.json").write_text(json.dumps({
            "base_commit": BASE,
            "patches": [{"file": name, "sha256": sha(patch_dir / name)} for name in PATCH_NAMES],
            "patched_commit": run(["git", "rev-parse", "HEAD"], isolated).strip(),
            "patched_tree_clean": True,
            "sqlite_control_passed": "test result: ok." in test_output,
            "capture_process": "dedicated stdio process per fixture arm; viewer off; exclusive store; serial requests",
        }, indent=2) + "\n")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
        frozen.unlink(missing_ok=True)
        subprocess.run(
            ["git", "worktree", "remove", "--force", str(isolated)],
            cwd=repository,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


if __name__ == "__main__":
    main()
