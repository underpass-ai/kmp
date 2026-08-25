#!/usr/bin/env python3
"""Fail when KMP's MCP, skill, command, prompt, and docs inventories drift."""

from __future__ import annotations

import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
PLUGIN = ROOT / "plugins" / "kmp"
CONTRACT = json.loads((PLUGIN / "capabilities.json").read_text(encoding="utf-8"))

EXPECTED_TOOLS = {
    "kmp_ingest",
    "kmp_write_memory",
    "kmp_wake",
    "kmp_ask",
    "kmp_goto",
    "kmp_near",
    "kmp_rewind",
    "kmp_forward",
    "kmp_trace",
    "kmp_inspect",
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
if not isinstance(tools, list) or set(tools) != EXPECTED_TOOLS or len(tools) != 10:
    fail(f"MCP inventory differs: {tools!r}")

workflows = CONTRACT.get("human_workflows")
if not isinstance(workflows, list) or len(workflows) != 10:
    fail("human_workflows must contain exactly ten entries")
ids = {entry["id"] for entry in workflows}
if len(ids) != len(workflows):
    fail("human workflow ids are not unique")

claude = names(PLUGIN / "claude" / "commands")
prompts = names(PLUGIN / "codex" / "prompts", "kmp-")
codex_skills = {entry["codex_skill"] for entry in workflows}
native_skills = {
    path.parent.name for path in (PLUGIN / "skills").glob("*/SKILL.md")
}

if claude != ids:
    fail(f"Claude commands differ: contract={sorted(ids)}, files={sorted(claude)}")
if prompts != ids:
    fail(f"standalone prompts differ: contract={sorted(ids)}, files={sorted(prompts)}")
if native_skills != codex_skills | {"kmp-memory"}:
    fail(
        "native Codex skills differ: "
        f"contract={sorted(codex_skills | {'kmp-memory'})}, files={sorted(native_skills)}"
    )

for entry in workflows:
    if entry["claude_command"] != entry["id"]:
        fail(f"{entry['id']} has a mismatched Claude exposure")
    if entry["codex_standalone_prompt"] != f"kmp-{entry['id']}":
        fail(f"{entry['id']} has a mismatched standalone prompt")
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

installer = (ROOT / "scripts/mcp/install-kmp-plugin.sh").read_text(encoding="utf-8")
doctor = (PLUGIN / "scripts/kmp-doctor.sh").read_text(encoding="utf-8")
for label, text, variable in (
    ("installer", installer, "CODEX_PROMPTS"),
    ("doctor", doctor, "CODEX_PROMPT_NAMES"),
):
    match = re.search(rf'(?m)^\s*{variable}="([^"]+)"$', text)
    if not match:
        fail(f"{label} does not declare {variable}")
    declared = {name.removeprefix("kmp-") for name in match.group(1).split()}
    if declared != ids:
        fail(f"{label} workflow list differs: {sorted(declared)}")

readme = (PLUGIN / "README.md").read_text(encoding="utf-8")
table_names = set(re.findall(r"(?m)^\| `/kmp:([a-z]+)` \|", readme))
if table_names != ids:
    fail(f"README workflow table differs: {sorted(table_names)}")
if "For you — ten commands" not in readme:
    fail("README does not state the ten-command contract")

manifest = json.loads((PLUGIN / ".codex-plugin/plugin.json").read_text(encoding="utf-8"))
if manifest.get("skills") != "./skills/":
    fail("Codex manifest does not expose the native skills directory")

codex_assets = [
    *(PLUGIN / "skills").glob("**/*"),
    *(PLUGIN / "codex").glob("**/*"),
    PLUGIN / ".codex-plugin/plugin.json",
]
for asset in codex_assets:
    if asset.is_file() and "CLAUDE_PLUGIN_ROOT" in asset.read_text(encoding="utf-8"):
        fail(f"Codex asset references CLAUDE_PLUGIN_ROOT: {asset.relative_to(ROOT)}")

retired = re.compile(
    r"\bkernel_(?:ingest|write_memory|wake|ask|goto|near|rewind|forward|trace|inspect)\b"
)
living = [
    PLUGIN / "README.md",
    *(PLUGIN / "claude" / "commands").glob("*.md"),
    *(PLUGIN / "codex" / "prompts").glob("*.md"),
    *(PLUGIN / "skills").glob("*/SKILL.md"),
]
for asset in living:
    if retired.search(asset.read_text(encoding="utf-8")):
        fail(f"living instruction uses a retired tool name: {asset.relative_to(ROOT)}")

print("KMP capability contract passed: 10 MCP tools, 10 workflows, 11 native skills")
