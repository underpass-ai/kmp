#!/usr/bin/env python3
"""Keep release-critical JavaScript actions on reviewed Node.js 24 pins."""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"
APPROVED = {
    "actions/upload-artifact": "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    "actions/download-artifact": "3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
    "Swatinem/rust-cache": "6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
}
USES = re.compile(r"^\s*uses:\s*([^\s@]+)@([^\s#]+)")


failures: list[str] = []
seen = {action: 0 for action in APPROVED}
for path in sorted((*WORKFLOWS.glob("*.yml"), *WORKFLOWS.glob("*.yaml"))):
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = USES.match(line)
        if not match:
            continue
        action, revision = match.groups()
        if action not in APPROVED:
            continue
        seen[action] += 1
        if revision != APPROVED[action]:
            failures.append(
                f"{path.relative_to(ROOT)}:{number}: {action}@{revision}; "
                f"expected reviewed Node.js 24 pin {APPROVED[action]}"
            )

for action, count in seen.items():
    if count == 0:
        failures.append(f"no workflow references the guarded action {action}")

if failures:
    raise SystemExit("GitHub Actions Node.js 24 gate:\n" + "\n".join(failures))

print(
    "GitHub Actions Node.js 24 gate passed: "
    + ", ".join(f"{action} ({seen[action]})" for action in APPROVED)
)
