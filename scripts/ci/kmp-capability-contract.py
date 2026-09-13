#!/usr/bin/env python3
"""Check the packaged MCP, skill and Claude command inventories."""

from __future__ import annotations

import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
PLUGIN = ROOT / "plugins" / "kmp"
CONTRACT = json.loads((PLUGIN / "capabilities.json").read_text(encoding="utf-8"))

# Memory and semantic viewer tools exposed by the package.
# The view tools are read-only with respect to memory by construction.
EXPECTED_TOOLS = {
    "kmp_guide",
    "kmp_ingest",
    "kmp_write_memory",
    "kmp_wake",
    "kmp_ask",
    "kmp_relate",
    "kmp_goto",
    "kmp_near",
    "kmp_rewind",
    "kmp_forward",
    "kmp_trace",
    "kmp_inspect",
    "kmp_relabel",
    "kmp_condense",
    "kmp_summaries_audit",
    "kmp_view_open",
    "kmp_view_apply_intent",
    "kmp_view_get_state",
}


def fail(message: str) -> None:
    raise SystemExit(f"KMP capability contract: {message}")


def names(directory: pathlib.Path, prefix: str = "") -> set[str]:
    return {
        path.stem.removeprefix(prefix)
        for path in directory.glob("*.md")
        if path.is_file()
    }


tools = CONTRACT.get("mcp_tools")
if not isinstance(tools, list) or set(tools) != EXPECTED_TOOLS or len(tools) != len(EXPECTED_TOOLS):
    fail(f"MCP inventory differs: {tools!r}")

workflows = CONTRACT.get("human_workflows")
if not isinstance(workflows, list) or len(workflows) != 10:
    fail("human_workflows must contain exactly ten entries")
ids = {entry["id"] for entry in workflows}
if len(ids) != len(workflows):
    fail("human workflow ids are not unique")

claude = names(PLUGIN / "claude" / "commands")
codex_skills = {entry["codex_skill"] for entry in workflows}
native_skills = {
    path.parent.name for path in (PLUGIN / "skills").glob("*/SKILL.md")
}

if claude != ids:
    fail(f"Claude commands differ: contract={sorted(ids)}, files={sorted(claude)}")
agent_skills = {entry["codex"] for entry in CONTRACT["agent_workflows"]}
if native_skills != codex_skills | agent_skills:
    fail(
        "native Codex skills differ: "
        f"contract={sorted(codex_skills | agent_skills)}, files={sorted(native_skills)}"
    )

for entry in workflows:
    if entry["claude_command"] != entry["id"]:
        fail(f"{entry['id']} has a mismatched Claude exposure")
    implementation = PLUGIN / entry["implementation"]
    if not implementation.is_file():
        fail(f"{entry['id']} implementation is missing: {implementation}")

for skill in sorted((PLUGIN / "skills").glob("*/SKILL.md")):
    text = skill.read_text(encoding="utf-8")
    match = re.search(r"(?m)^name:\s*([^\s]+)\s*$", text)
    if not match or match.group(1) != skill.parent.name:
        fail(f"skill name does not match its directory: {skill}")

# Codex currently attempts best-effort conversion of simple Claude commands.
# Every command is deliberately parameterized so Codex consumes the native
# skills above instead of materializing an accidental partial second surface.
for command in sorted((PLUGIN / "claude" / "commands").glob("*.md")):
    if not re.search(r"(?m)^argument-hint:\s*", command.read_text(encoding="utf-8")):
        fail(f"{command.name} can be accidentally auto-migrated by Codex")

manifest = json.loads((PLUGIN / ".codex-plugin/plugin.json").read_text(encoding="utf-8"))
if manifest.get("skills") != "./skills/":
    fail("Codex manifest does not expose the native skills directory")

codex_assets = [
    *(PLUGIN / "skills").glob("**/*"),
    PLUGIN / ".codex-plugin/plugin.json",
]
for asset in codex_assets:
    if asset.is_file() and "CLAUDE_PLUGIN_ROOT" in asset.read_text(encoding="utf-8"):
        fail(f"Codex asset references CLAUDE_PLUGIN_ROOT: {asset.relative_to(ROOT)}")

print(
    f"KMP capability contract passed: {len(tools)} MCP tools, "
    f"{len(workflows)} workflows, {len(native_skills)} native skills"
)
