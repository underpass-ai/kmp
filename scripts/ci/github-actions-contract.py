#!/usr/bin/env python3
"""Keep every external GitHub Action on a reviewed immutable runtime pin."""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
GITHUB = ROOT / ".github"
APPROVED = {
    "actions/checkout": "3d3c42e5aac5ba805825da76410c181273ba90b1",
    "actions/upload-artifact": "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    "actions/download-artifact": "3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
    "actions/github-script": "3a2844b7e9c422d3c10d287c895573f7108da1b3",
    "actions/dependency-review-action": "a1d282b36b6f3519aa1f3fc636f609c47dddb294",
    "Swatinem/rust-cache": "6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
    "docker/setup-buildx-action": "37fe631027851001ddb9b187196cc803df7f5f0e",
    "docker/login-action": "dbcb813823bdd20940b903addbd779551569679f",
    "docker/metadata-action": "dc802804100637a589fabce1cb79ff13a1411302",
    "docker/build-push-action": "53b7df96c91f9c12dcc8a07bcb9ccacbed38856a",
    "taiki-e/install-action": "fcf5432d9f50d67e37ee6e29bdb7a224ff67b4a7",
}
USES = re.compile(r"^\s*uses:\s*([^\s@]+)@([^\s#]+)")


failures: list[str] = []
seen = {action: 0 for action in APPROVED}
paths = sorted((*GITHUB.rglob("*.yml"), *GITHUB.rglob("*.yaml")))
for path in paths:
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = USES.match(line)
        if not match:
            continue
        action, revision = match.groups()
        if action not in APPROVED:
            failures.append(
                f"{path.relative_to(ROOT)}:{number}: unreviewed external action "
                f"{action}@{revision}"
            )
            continue
        seen[action] += 1
        if revision != APPROVED[action]:
            failures.append(
                f"{path.relative_to(ROOT)}:{number}: {action}@{revision}; "
                f"expected reviewed immutable pin {APPROVED[action]}"
            )

for action, count in seen.items():
    if count == 0:
        failures.append(f"no workflow references the guarded action {action}")

if failures:
    raise SystemExit("GitHub Actions contract failed:\n" + "\n".join(failures))

print(
    "GitHub Actions contract passed: "
    + ", ".join(f"{action} ({seen[action]})" for action in APPROVED)
)
