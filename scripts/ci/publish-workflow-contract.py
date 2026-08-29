#!/usr/bin/env python3
"""Keep tag-only publication and immutable candidate promotion honest."""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys
import tempfile


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
if 'verify-marketplace.py "${GITHUB_REF_NAME#v}"' not in publish_text:
    raise SystemExit("publish-distribution can publish a tag before marketplace parity")
if 'changelog.py check "${version}"' not in publish_text:
    raise SystemExit("publish-distribution can publish a tag without versioned notes")
if "sync-public-readme.py check" not in publish_text:
    raise SystemExit("publish-distribution can publish divergent public READMEs")
if 'guide.py check "${version}"' not in publish_text:
    raise SystemExit("publish-distribution can publish a stale guide envelope")
if publish_text.count("needs: verify-release") != 3:
    raise SystemExit("every distribution publisher must depend on release readiness")

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
    'verify-marketplace.py "${{ steps.candidate.outputs.version }}"',
    'changelog.py check "${{ steps.candidate.outputs.version }}"',
    "sync-public-readme.py check",
    'guide.py check "${{ steps.candidate.outputs.version }}"',
    "build-guide.py check --binary",
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
    'verify-marketplace.py "${version}"',
    'changelog.py prepare "${version}"',
    'changelog.py check "${version}"',
    "sync-public-readme.py sync",
    "sync-public-readme.py check",
    'guide.py sync "${version}" --binary target/debug/kmp-mcp',
    'guide.py check "${version}"',
):
    if clause not in release_script:
        raise SystemExit(f"release helper lost candidate approval clause: {clause}")

scratch_root = ROOT / "tmp"
scratch_root.mkdir(exist_ok=True)
marketplace_verifier = ROOT / "scripts/release/verify-marketplace.py"
with tempfile.TemporaryDirectory(dir=scratch_root) as raw_fixture:
    fixture = pathlib.Path(raw_fixture)
    current_description = (
        "Local-first agent memory: teaches the memory moves and opens memory in ChronoLoom."
    )
    source_root = fixture / "release-source"
    plugin_root = fixture / "plugins/kmp"
    claude_root = fixture / "claude-source"
    manifests = {
        "source_claude": source_root / ".claude-plugin/plugin.json",
        "source_codex": source_root / ".codex-plugin/plugin.json",
        "snapshot_claude": plugin_root / ".claude-plugin/plugin.json",
        "codex": plugin_root / ".codex-plugin/plugin.json",
        "claude": claude_root / ".claude-plugin/plugin.json",
        "claude_codex": claude_root / ".codex-plugin/plugin.json",
    }
    for manifest in manifests.values():
        manifest.parent.mkdir(parents=True, exist_ok=True)
        manifest.write_text(
            json.dumps({"version": "0.4.2", "description": current_description}),
            encoding="utf-8",
        )
    for root in (source_root, plugin_root, claude_root):
        skill = root / "skills/kmp-memory/SKILL.md"
        skill.parent.mkdir(parents=True)
        skill.write_text("Recover before re-deriving.\n", encoding="utf-8")
    claude_listing = fixture / "claude-marketplace.json"
    claude_listing.write_text(
        json.dumps(
            {
                "plugins": [
                    {
                        "name": "kmp",
                        "description": current_description,
                        "source": {
                            "source": "git-subdir",
                            "url": "https://github.com/underpass-ai/kmp.git",
                            "path": "plugins/kmp",
                            "ref": "a" * 40,
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    codex_listing = fixture / "codex-marketplace.json"
    codex_listing.write_text(
        json.dumps(
            {
                "plugins": [
                    {
                        "name": "kmp",
                        "source": {"source": "local", "path": "./plugins/kmp"},
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    readme = fixture / "README.md"
    readme.write_text(
        "| `kmp` | product | snapshot | Local-first agent memory over thirteen MCP tools: "
        "ten memory moves plus three shared ChronoLoom view tools. |\n",
        encoding="utf-8",
    )

    verify = [
        sys.executable,
        marketplace_verifier,
        "0.4.2",
        "--root",
        plugin_root,
        "--listing",
        claude_listing,
        "--codex-listing",
        codex_listing,
        "--readme",
        readme,
        "--claude-root",
        claude_root,
        "--source-root",
        source_root,
    ]

    subprocess.run(
        verify,
        check=True,
        stdout=subprocess.DEVNULL,
    )

    manifests["claude"].write_text(
        json.dumps({"version": "0.4.2+cache.1", "description": current_description}),
        encoding="utf-8",
    )
    subprocess.run(
        verify,
        check=True,
        stdout=subprocess.DEVNULL,
    )

    manifests["codex"].write_text(
        json.dumps({"version": "0.4.1", "description": current_description}),
        encoding="utf-8",
    )
    stale = subprocess.run(
        verify,
        check=False,
        capture_output=True,
        text=True,
    )
    if stale.returncode == 0 or "merge the underpass-ai/plugins mirror PR" not in stale.stderr:
        raise SystemExit("marketplace verifier accepted a stale host manifest")

    manifests["codex"].write_text(
        json.dumps({"version": "0.4.2", "description": current_description}),
        encoding="utf-8",
    )
    manifests["claude"].write_text(
        json.dumps({"version": "0.4.1", "description": current_description}),
        encoding="utf-8",
    )
    stale = subprocess.run(verify, check=False, capture_output=True, text=True)
    if stale.returncode == 0 or "for Claude is '0.4.1'" not in stale.stderr:
        raise SystemExit("marketplace verifier read the mirrored manifest instead of Claude's source")

    manifests["claude"].write_text(
        json.dumps({"version": "0.4.2", "description": current_description}),
        encoding="utf-8",
    )
    claude_listing.write_text(
        json.dumps(
            {
                "plugins": [
                    {
                        "name": "kmp",
                        "description": "Teaches the ten moves and diagnoses setup.",
                        "source": {
                            "source": "git-subdir",
                            "url": "https://github.com/underpass-ai/kmp.git",
                            "path": "plugins/kmp",
                            "ref": "a" * 40,
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    stale_copy = subprocess.run(verify, check=False, capture_output=True, text=True)
    if stale_copy.returncode == 0 or "ChronoLoom" not in stale_copy.stderr:
        raise SystemExit("marketplace verifier accepted stale whole-surface copy")

    claude_listing.write_text(
        json.dumps(
            {
                "plugins": [
                    {
                        "name": "kmp",
                        "description": current_description,
                        "source": {
                            "source": "git-subdir",
                            "url": "https://github.com/underpass-ai/kmp.git",
                            "path": "plugins/kmp",
                            "ref": "a" * 40,
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    readme.write_text(
        "| `kmp` | product | snapshot | Navigable memory: 10 KMP moves, with ChronoLoom. |\n",
        encoding="utf-8",
    )
    stale_copy = subprocess.run(verify, check=False, capture_output=True, text=True)
    if stale_copy.returncode == 0 or "retired ten-move" not in stale_copy.stderr:
        raise SystemExit("marketplace verifier accepted the stale public README product row")

    readme.write_text(
        "| `kmp` | product | snapshot | Local-first agent memory over thirteen MCP tools: "
        "ten memory moves plus three shared ChronoLoom view tools. |\n",
        encoding="utf-8",
    )
    claude_listing.write_text(
        json.dumps(
            {
                "plugins": [
                    {
                        "name": "kmp",
                        "description": current_description,
                        "source": {
                            "source": "git-subdir",
                            "url": "https://github.com/underpass-ai/kmp.git",
                            "path": "plugins/kmp",
                            "ref": "main",
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    moving_ref = subprocess.run(verify, check=False, capture_output=True, text=True)
    if moving_ref.returncode == 0 or "immutable 40-character commit SHA" not in moving_ref.stderr:
        raise SystemExit("marketplace verifier accepted a moving Claude source ref")

    listing_body = json.loads(claude_listing.read_text(encoding="utf-8"))
    listing_body["plugins"][0]["source"]["ref"] = "a" * 40
    listing_body["plugins"][0]["source"]["path"] = "plugin/kmp"
    claude_listing.write_text(json.dumps(listing_body), encoding="utf-8")
    wrong_mapping = subprocess.run(verify, check=False, capture_output=True, text=True)
    if wrong_mapping.returncode == 0 or "no longer resolves" not in wrong_mapping.stderr:
        raise SystemExit("marketplace verifier accepted the wrong Claude source path")
    listing_body["plugins"][0]["source"]["path"] = "plugins/kmp"
    claude_listing.write_text(json.dumps(listing_body), encoding="utf-8")

    codex_body = json.loads(codex_listing.read_text(encoding="utf-8"))
    codex_body["plugins"][0]["source"]["path"] = "./plugin/kmp"
    codex_listing.write_text(json.dumps(codex_body), encoding="utf-8")
    wrong_mapping = subprocess.run(verify, check=False, capture_output=True, text=True)
    if wrong_mapping.returncode == 0 or "reviewed plugins/kmp snapshot" not in wrong_mapping.stderr:
        raise SystemExit("marketplace verifier accepted the wrong Codex source path")
    codex_body["plugins"][0]["source"]["path"] = "./plugins/kmp"
    codex_listing.write_text(json.dumps(codex_body), encoding="utf-8")

    claude_skill = claude_root / "skills/kmp-memory/SKILL.md"
    claude_skill.write_text("Stale routing.\n", encoding="utf-8")
    stale_tree = subprocess.run(verify, check=False, capture_output=True, text=True)
    if stale_tree.returncode == 0 or "Claude marketplace source" not in stale_tree.stderr:
        raise SystemExit("marketplace verifier accepted stale Claude content at the same version")
    claude_skill.write_text("Recover before re-deriving.\n", encoding="utf-8")

    codex_skill = plugin_root / "skills/kmp-memory/SKILL.md"
    codex_skill.write_text("Stale routing.\n", encoding="utf-8")
    stale_tree = subprocess.run(verify, check=False, capture_output=True, text=True)
    if stale_tree.returncode == 0 or "Codex marketplace snapshot" not in stale_tree.stderr:
        raise SystemExit("marketplace verifier accepted stale Codex content at the same version")

guide_verifier = ROOT / "scripts/release/guide.py"
with tempfile.TemporaryDirectory(dir=scratch_root) as raw_fixture:
    fixture = pathlib.Path(raw_fixture)
    (fixture / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "0.4.2"\n', encoding="utf-8"
    )
    guide = fixture / "plugins/kmp/guide"
    guide.mkdir(parents=True)
    for relative in (".claude-plugin/plugin.json", ".codex-plugin/plugin.json"):
        manifest = fixture / "plugins/kmp" / relative
        manifest.parent.mkdir(parents=True, exist_ok=True)
        manifest.write_text(json.dumps({"version": "0.4.2"}), encoding="utf-8")
    (guide / "editorial.json").write_text(
        json.dumps({"guide_version": "1"}), encoding="utf-8"
    )
    requests = [
        {
            "about": about,
            "idempotency_key": f"ingest:guide-sync:1:{audience}:fixture",
            "memory": {
                "entries": [
                    {"metadata": {"guide_version": "1"}}
                ]
            },
        }
        for about, audience in (("guide:kmp-agent", "agent"), ("guide:kmp", "person"))
    ]
    (guide / "guide.requests.json").write_text(
        json.dumps(requests), encoding="utf-8"
    )
    bundle_header = {
        "bundle_format": 2,
        "event_count": 2,
        "kernel_version": "0.4.2",
        "abouts": ["guide:kmp", "guide:kmp-agent"],
    }
    (guide / "memory.jsonl").write_text(
        json.dumps(bundle_header) + "\n", encoding="utf-8"
    )
    check_guide = [
        sys.executable,
        guide_verifier,
        "check",
        "0.4.2",
        "--root",
        fixture,
    ]
    subprocess.run(check_guide, check=True, stdout=subprocess.DEVNULL)
    bundle_header["kernel_version"] = "0.4.1"
    (guide / "memory.jsonl").write_text(
        json.dumps(bundle_header) + "\n", encoding="utf-8"
    )
    stale_guide = subprocess.run(
        check_guide, check=False, capture_output=True, text=True
    )
    if stale_guide.returncode == 0 or "not release '0.4.2'" not in stale_guide.stderr:
        raise SystemExit("release guide verifier accepted a stale engine envelope")

changelog_helper = ROOT / "scripts/release/changelog.py"
with tempfile.TemporaryDirectory(dir=scratch_root) as raw_fixture:
    fixture = pathlib.Path(raw_fixture) / "CHANGELOG.md"
    empty = """# Changelog

## [Unreleased]

## [0.4.2] - 2026-08-28

### Fixed

- Existing release.

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.4.2
"""
    fixture.write_text(empty, encoding="utf-8")
    prepare = [
        sys.executable,
        changelog_helper,
        "prepare",
        "0.4.3",
        "--path",
        fixture,
        "--date",
        "2026-08-29",
    ]
    rejected = subprocess.run(prepare, check=False, capture_output=True, text=True)
    if rejected.returncode == 0 or "[Unreleased] is empty" not in rejected.stderr:
        raise SystemExit("release preparation accepted an empty changelog")
    if fixture.read_text(encoding="utf-8") != empty:
        raise SystemExit("failed changelog preparation changed the fixture")

    populated = empty.replace(
        "## [Unreleased]\n\n",
        "## [Unreleased]\n\n### Added\n\n- A documented change.\n\n",
        1,
    )
    fixture.write_text(populated, encoding="utf-8")
    subprocess.run(prepare, check=True, stdout=subprocess.DEVNULL)
    prepared = fixture.read_text(encoding="utf-8")
    if (
        "## [0.4.3] - 2026-08-29" not in prepared
        or "- A documented change." not in prepared
    ):
        raise SystemExit("release preparation did not promote Unreleased notes")
    subprocess.run(
        [sys.executable, changelog_helper, "check", "0.4.3", "--path", fixture],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    subprocess.run(prepare, check=True, stdout=subprocess.DEVNULL)
    if fixture.read_text(encoding="utf-8") != prepared:
        raise SystemExit("changelog preparation is not idempotent")

readme_helper = ROOT / "scripts/release/sync-public-readme.py"
with tempfile.TemporaryDirectory(dir=scratch_root) as raw_fixture:
    fixture = pathlib.Path(raw_fixture)
    source = fixture / "marketplace.md"
    github = fixture / "github.md"
    crate = fixture / "crate.md"
    begin = "<!-- kmp:public-overview:begin -->"
    end = "<!-- kmp:public-overview:end -->"
    overview = """KMP gives Codex and Claude Code local-first memory. It stores
decisions and evidence on embedded SQLite, not transcripts, through ten memory
tools plus three semantic view tools over a shared ChronoLoom view."""
    source.write_text(
        f"marketplace header\n{begin}\n{overview}\n{end}\nmarketplace tail\n",
        encoding="utf-8",
    )
    github.write_text(
        f"github header\n{begin}\nStale overview.\n{end}\ngithub tail\n",
        encoding="utf-8",
    )
    crate.write_text(
        f"crate header\n{begin}\n{overview}\n{end}\ncrate tail\n",
        encoding="utf-8",
    )
    readme_args = [
        "--source",
        source,
        "--target",
        github,
        "--target",
        crate,
    ]
    rejected = subprocess.run(
        [sys.executable, readme_helper, "check", *readme_args],
        check=False,
        capture_output=True,
        text=True,
    )
    if rejected.returncode == 0 or "stale generated overview" not in rejected.stderr:
        raise SystemExit("public README contract accepted a stale GitHub surface")
    subprocess.run(
        [sys.executable, readme_helper, "sync", *readme_args],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    synchronized = (github.read_text(encoding="utf-8"), crate.read_text(encoding="utf-8"))
    subprocess.run(
        [sys.executable, readme_helper, "check", *readme_args],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    subprocess.run(
        [sys.executable, readme_helper, "sync", *readme_args],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    if synchronized != (
        github.read_text(encoding="utf-8"),
        crate.read_text(encoding="utf-8"),
    ):
        raise SystemExit("public README synchronization is not idempotent")

    source.write_text(
        source.read_text(encoding="utf-8").replace("embedded SQLite", "a store"),
        encoding="utf-8",
    )
    rejected = subprocess.run(
        [sys.executable, readme_helper, "check", *readme_args],
        check=False,
        capture_output=True,
        text=True,
    )
    if rejected.returncode == 0 or "embedded SQLite engine" not in rejected.stderr:
        raise SystemExit("public README contract accepted an incomplete product story")

print("release trigger contract passed: PR validation, one candidate build, tag-only promotion")

mcp_registry_text = (ROOT / ".github/workflows/mcp-registry.yml").read_text(encoding="utf-8")
for clause in (
    "github.event_name == 'workflow_dispatch'",
    "scripts/release/verify-marketplace.py",
    "scripts/release/changelog.py",
    "scripts/release/sync-public-readme.py",
    "VERSION=\"$(jq -r '.version' server.json)\"",
    "TAG=\"v${VERSION}\"",
    "-A 'kmp-mcp-release-check/0.1 (+https://github.com/underpass-ai/kmp)'",
    'gh release view "${TAG}"',
):
    if clause not in mcp_registry_text:
        raise SystemExit(f"MCP Registry recovery contract lost clause: {clause}")

print("MCP Registry recovery contract passed: tag and manual OIDC publication")
