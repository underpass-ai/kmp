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


def run(
    arguments: list[str],
    cwd: Path,
    evidence: Path,
    commands: list[dict],
    env: dict[str, str] | None = None,
) -> str:
    completed = subprocess.run(
        arguments,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    record = {
        "sequence": len(commands) + 1,
        "arguments": arguments,
        "cwd": str(cwd),
        "exit_code": completed.returncode,
        "output": completed.stdout,
    }
    commands.append(record)
    (evidence / f"command-{record['sequence']:03}.json").write_text(
        json.dumps(record, ensure_ascii=False, indent=2) + "\n"
    )
    if completed.returncode != 0:
        raise subprocess.CalledProcessError(
            completed.returncode, arguments, output=completed.stdout
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
    output.mkdir(parents=True)
    commands: list[dict] = []
    status = {
        "status": "running",
        "base_commit": BASE,
        "commands": [],
    }

    def save_status() -> None:
        status["commands"] = [
            {key: row[key] for key in ["sequence", "arguments", "cwd", "exit_code"]}
            for row in commands
        ]
        (output / "patch-validation.json").write_text(
            json.dumps(status, ensure_ascii=False, indent=2) + "\n"
        )

    save_status()

    patch_dir = repository / "artifacts/proof-batch-physical-20260915/instrumentation-patches"
    isolated: Path | None = None
    frozen: Path | None = None
    test_output = ""
    try:
        manifest = json.loads((patch_dir / "manifest.json").read_text())
        expected = {item["file"]: item["sha256"] for item in manifest["patches"]}
        status["patches"] = []
        for name in PATCH_NAMES:
            actual = sha(patch_dir / name)
            status["patches"].append({
                "file": name,
                "expected_sha256": expected[name],
                "actual_sha256": actual,
                "matches": actual == expected[name],
            })
            if actual != expected[name]:
                raise RuntimeError(f"instrumentation patch hash changed: {name}")

        if run(
            ["git", "status", "--porcelain"], repository, output, commands
        ).strip():
            raise RuntimeError("repository must be clean before creating the isolated worktree")
        run(["git", "cat-file", "-e", f"{BASE}^{{commit}}"], repository, output, commands)
        run(
            ["git", "merge-base", "--is-ancestor", BASE, "origin/main"],
            repository,
            output,
            commands,
        )
        repository_tmp = repository / "tmp"
        repository_tmp.mkdir(exist_ok=True)
        isolated = Path(tempfile.mkdtemp(prefix="proof-539-patched-", dir=repository_tmp))
        frozen = isolated.parent / f"{isolated.name}-binary"
        isolated.rmdir()
        run(
            ["git", "worktree", "add", "--detach", str(isolated), BASE],
            repository,
            output,
            commands,
        )
        for name in PATCH_NAMES:
            run(["git", "am", str(patch_dir / name)], isolated, output, commands)
        if run(
            ["git", "status", "--porcelain"], isolated, output, commands
        ).strip():
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
            output,
            commands,
            enabled_env,
        )
        run(
            ["cargo", "build", "--locked", "-p", "kmp-mcp"],
            isolated,
            output,
            commands,
        )
        metadata = json.loads(
            run(
                ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                isolated,
                output,
                commands,
            )
        )
        shutil.copy2(Path(metadata["target_directory"]) / "debug/kmp-mcp", frozen)
        run(
            [
                sys.executable,
                str(repository / "scripts/performance/proof_batch_acceptance.py"),
                str(frozen), str(output / "capture"), str(scratch), samples,
            ],
            repository,
            output,
            commands,
        )
        run(
            [
                sys.executable,
                str(repository / "scripts/performance/proof_batch_physical_audit.py"),
                str(output / "capture"), str(output / "capture/physical-summary.json"),
            ],
            repository,
            output,
            commands,
        )
        status.update({
            "status": "success",
            "patched_commit": run(
                ["git", "rev-parse", "HEAD"], isolated, output, commands
            ).strip(),
            "patched_tree_clean": True,
            "sqlite_control_passed": "test result: ok." in test_output,
            "capture_process": "dedicated stdio process per fixture arm; viewer off; exclusive store; serial requests",
        })
    except BaseException as error:
        status.update({
            "status": "failed",
            "error_type": type(error).__name__,
            "error": str(error),
        })
        raise
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
        if frozen is not None:
            frozen.unlink(missing_ok=True)
        if isolated is not None:
            cleanup = subprocess.run(
                ["git", "worktree", "remove", "--force", str(isolated)],
                cwd=repository,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            cleanup_record = {
                "arguments": ["git", "worktree", "remove", "--force", str(isolated)],
                "cwd": str(repository),
                "exit_code": cleanup.returncode,
                "output": cleanup.stdout,
            }
            (output / "cleanup.json").write_text(
                json.dumps(cleanup_record, ensure_ascii=False, indent=2) + "\n"
            )
            status["cleanup_exit_code"] = cleanup.returncode
        save_status()


if __name__ == "__main__":
    main()
