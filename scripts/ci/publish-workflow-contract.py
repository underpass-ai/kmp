#!/usr/bin/env python3
"""Keep tag-only publication and immutable candidate promotion honest."""

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

publish_text = WORKFLOW.read_text(encoding="utf-8")
publish_trigger = publish_text.split("\nenv:", 1)[0]
if "branches:" in publish_trigger or "- main" in publish_trigger:
    raise SystemExit("publish-distribution must not run automatically on main")
if 'tags:\n      - "v*"' not in publish_trigger:
    raise SystemExit("publish-distribution lost its version-tag trigger")

plugin_text = (ROOT / ".github/workflows/plugin-package.yml").read_text(encoding="utf-8")
plugin_trigger = plugin_text.split("\nenv:", 1)[0]
for forbidden in ("\n  push:", "\n  workflow_dispatch:"):
    if forbidden in plugin_trigger:
        raise SystemExit("plugin-package must validate pull requests without publishing elsewhere")

release_text = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
release_clauses = (
    "if: github.event_name == 'workflow_dispatch'",
    "promote reviewed candidate",
    "candidate-run:",
    "run-id: ${{ steps.candidate.outputs.run_id }}",
    "release-candidate.py verify",
    "dist/candidate/assets/* --clobber",
)
for clause in release_clauses:
    if clause not in release_text:
        raise SystemExit(f"release workflow lost immutable promotion clause: {clause}")

release_script = (ROOT / "scripts/release.sh").read_text(encoding="utf-8")
for clause in (
    "candidate <X.Y.Z> [RUN_ID]",
    "gh workflow run release.yml",
    "scripts/release/stamp-server-mcpb.sh",
    "kmp-release-candidate-${version}",
    "release-candidate.py verify",
    "candidate-run: ${candidate_run}",
):
    if clause not in release_script:
        raise SystemExit(f"release helper lost candidate approval clause: {clause}")

print("release trigger contract passed: PR validation, one candidate build, tag-only promotion")

mcp_registry_text = (ROOT / ".github/workflows/mcp-registry.yml").read_text(encoding="utf-8")
for clause in (
    "github.event_name == 'workflow_dispatch'",
    "VERSION=\"$(jq -r '.version' server.json)\"",
    "TAG=\"v${VERSION}\"",
    "-A 'kmp-mcp-release-check/0.1 (+https://github.com/underpass-ai/kmp)'",
    'gh release view "${TAG}"',
):
    if clause not in mcp_registry_text:
        raise SystemExit(f"MCP Registry recovery contract lost clause: {clause}")

print("MCP Registry recovery contract passed: tag and manual OIDC publication")
