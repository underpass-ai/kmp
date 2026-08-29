#!/usr/bin/env python3
"""Keep tag-only publication and Rust-owned release contracts honest."""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]


def require(text: str, clauses: tuple[str, ...], label: str) -> None:
    for clause in clauses:
        if clause not in text:
            raise SystemExit(f"{label} lost required clause: {clause}")


def reject(text: str, clauses: tuple[str, ...], label: str) -> None:
    for clause in clauses:
        if clause in text:
            raise SystemExit(f"{label} retains forbidden clause: {clause}")


publish_path = ROOT / ".github/workflows/publish-distribution.yml"
publish_text = publish_path.read_text(encoding="utf-8")
publish_trigger = publish_text.split("\nenv:", 1)[0]
if "branches:" in publish_trigger or "- main" in publish_trigger:
    raise SystemExit("publish-distribution must not run automatically on main")
if 'tags:\n      - "v*"' not in publish_trigger:
    raise SystemExit("publish-distribution lost its version-tag trigger")
require(
    publish_text,
    (
        "cargo run --locked --quiet -p kmp-release -- changelog check",
        "marketplace verify \"${GITHUB_REF_NAME#v}\"",
    ),
    "publish-distribution",
)
reject(
    publish_text,
    ("scripts/release/", "marketplace-commit", "underpass-ai/plugins"),
    "publish-distribution",
)
if publish_text.count("needs: verify-release") != 3:
    raise SystemExit("every distribution publisher must depend on release readiness")

match = re.search(
    r"(?ms)^  publish-crates:.*?(?=^  [a-zA-Z][a-zA-Z0-9_-]*:|\Z)", publish_text
)
if match is None:
    raise SystemExit("publish-distribution has no publish-crates job")
cache_match = re.search(
    r"(?ms)- name: Cache Rust build artifacts.*?(?=^      - name:|\Z)", match.group()
)
if cache_match is None:
    raise SystemExit("publish-crates must declare its reviewed Rust cache policy")
require(
    cache_match.group(),
    ("cache-targets: false", "save-if: false"),
    "publish-crates cache",
)

plugin_text = (ROOT / ".github/workflows/plugin-package.yml").read_text(
    encoding="utf-8"
)
plugin_trigger = plugin_text.split("\nenv:", 1)[0]
reject(plugin_trigger, ("\n  push:", "\n  workflow_dispatch:"), "plugin-package")

release_text = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
require(
    release_text,
    (
        "if: github.event_name == 'workflow_dispatch'",
        "promote reviewed candidate",
        "candidate-run:",
        "run-id: ${{ steps.candidate.outputs.run_id }}",
        "candidate assemble",
        "candidate verify",
        "marketplace verify",
        "changelog check",
        "dist/candidate/assets/* --clobber",
    ),
    "release workflow",
)
reject(
    release_text,
    ("scripts/release/", "marketplace-commit", "underpass-ai/plugins"),
    "release workflow",
)

release_script = (ROOT / "scripts/release.sh").read_text(encoding="utf-8")
require(
    release_script,
    (
        "exec cargo run --locked --quiet",
        '-p kmp-release -- workflow "$@"',
    ),
    "release helper",
)
reject(
    release_script,
    (
        "gh ",
        "git ",
        "python",
        "cmd_candidate",
        "cmd_release",
        "cmd_version",
        "marketplace-commit",
        "underpass-ai/plugins",
        "release-candidate.py",
        "scripts/release/guide.py",
        "plugins/kmp/guide/build-guide.py",
        "scripts/release/package-kmp-mcpb.sh",
        "scripts/release/stamp-server-mcpb.sh",
        'python3 - "${version}"',
    ),
    "release helper",
)
if len(release_script.splitlines()) > 8:
    raise SystemExit("release helper must remain a thin Rust binary adapter")

release_rust = "\n".join(
    (ROOT / path).read_text(encoding="utf-8")
    for path in (
        "crates/kmp-release/src/application/use_cases/prepare_release_workflow.rs",
        "crates/kmp-release/src/application/use_cases/seal_release_candidate.rs",
        "crates/kmp-release/src/application/use_cases/publish_release_workflow.rs",
        "crates/kmp-release/src/adapters/gh_candidate_automation.rs",
        "crates/kmp-release/src/adapters/system_release_workspace.rs",
        "crates/kmp-release/src/adapters/current_binary_release_contracts.rs",
    )
)
require(
    release_rust,
    (
        "prepare_changelog(version)",
        "prepare_version(version)",
        "sync_guide(version",
        "candidate run:",
        "stamp_mcpb",
        "verify_candidate",
        "verify_marketplace",
        "allow-unpublished-tag",
        "candidate-run:",
        "workflow_dispatch",
        "create_and_push_tag",
    ),
    "Rust release workflow",
)

mcp_registry_text = (ROOT / ".github/workflows/mcp-registry.yml").read_text(
    encoding="utf-8"
)
require(
    mcp_registry_text,
    (
        "github.event_name == 'workflow_dispatch'",
        "cargo run --locked --quiet -p kmp-release -- changelog check",
        "marketplace verify",
        "VERSION=\"$(jq -r '.version' server.json)\"",
        'TAG="v${VERSION}"',
        "-A 'kmp-mcp-release-check/0.1 (+https://github.com/underpass-ai/kmp)'",
        'gh release view "${TAG}"',
    ),
    "MCP Registry recovery",
)
reject(mcp_registry_text, ("scripts/release/", "marketplace-commit"), "MCP Registry")

print(
    "release trigger contract passed: Rust release binary, co-located marketplace, "
    "one candidate build and tag-only promotion"
)
