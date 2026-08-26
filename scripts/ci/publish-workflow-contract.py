#!/usr/bin/env python3
"""Keep tag-only publication behavior free of misleading cache failures."""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/publish-distribution.yml"
lines = WORKFLOW.read_text(encoding="utf-8").splitlines()


def indented_block(start_pattern: str, indent: int) -> list[str]:
    start = next(
        (number for number, line in enumerate(lines) if re.fullmatch(start_pattern, line)),
        None,
    )
    if start is None:
        raise SystemExit(f"missing workflow block matching {start_pattern!r}")
    end = next(
        (
            number
            for number, line in enumerate(lines[start + 1 :], start + 1)
            if line.strip() and len(line) - len(line.lstrip()) <= indent
        ),
        len(lines),
    )
    return lines[start:end]


job = indented_block(r"  publish-crates:", 2)
cache_step_start = next(
    (
        number
        for number, line in enumerate(job)
        if line.strip() == "- name: Cache Rust build artifacts"
    ),
    None,
)
if cache_step_start is None:
    raise SystemExit("publish-crates must declare its reviewed Rust cache policy")

cache_step_end = next(
    (
        number
        for number, line in enumerate(job[cache_step_start + 1 :], cache_step_start + 1)
        if line.startswith("      - name:")
    ),
    len(job),
)
cache_step = "\n".join(job[cache_step_start:cache_step_end])

required = {
    "cache-targets: false": "must not restore mutable target artifacts",
    "save-if: false": "must not clean or save targets after cargo publish",
}
missing = [
    f"{setting} ({reason})"
    for setting, reason in required.items()
    if setting not in cache_step
]
if missing:
    raise SystemExit("publish-crates cache contract failed:\n" + "\n".join(missing))

print("publish-crates cache contract passed: registry restore only, no post-publish save")
