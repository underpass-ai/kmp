#!/usr/bin/env python3
"""Fail when KMP's MCP, skill, thin Claude adapters, and docs drift."""

from __future__ import annotations

import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
PLUGIN = ROOT / "plugins" / "kmp"
CONTRACT = json.loads((PLUGIN / "capabilities.json").read_text(encoding="utf-8"))

# Eleven moves over memory, and three over the view a person is looking at.
# The view tools are read-only with respect to memory by construction.
EXPECTED_TOOLS = {
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
if native_skills != codex_skills | {"kmp-memory"}:
    fail(
        "native Codex skills differ: "
        f"contract={sorted(codex_skills | {'kmp-memory'})}, files={sorted(native_skills)}"
    )

memory_skill = (PLUGIN / "skills" / "kmp-memory" / "SKILL.md").read_text(
    encoding="utf-8"
)
for tool in sorted(name for name in EXPECTED_TOOLS if name.startswith("kmp_view_")):
    if tool not in memory_skill:
        fail(f"kmp-memory does not route advertised view tool {tool}")

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

native_workflow_clauses = {
    "kmp-setup": ("scripts/kmp-doctor.sh", "scripts/kmp-update.sh"),
    "kmp-doctor": ("scripts/kmp-doctor.sh", "host wiring and ownership"),
    "kmp-guide": (
        "scripts/kmp-guide-sync.sh",
        "guide:kmp-agent",
        "guide:kmp",
        "open:guide",
        "kmp_view_open",
        "kmp_view_apply_intent",
        "kmp_view_get_state",
    ),
}
for skill_name, clauses in native_workflow_clauses.items():
    text = (PLUGIN / "skills" / skill_name / "SKILL.md").read_text(encoding="utf-8")
    for clause in clauses:
        if clause not in text:
            fail(f"{skill_name} lost its executable workflow clause: {clause}")

# Codex currently attempts best-effort conversion of simple Claude commands.
# Every command is deliberately parameterized so Codex consumes the native
# skills above instead of materializing an accidental partial second surface.
for command in sorted((PLUGIN / "claude" / "commands").glob("*.md")):
    if not re.search(r"(?m)^argument-hint:\s*", command.read_text(encoding="utf-8")):
        fail(f"{command.name} can be accidentally auto-migrated by Codex")

readme = (PLUGIN / "README.md").read_text(encoding="utf-8")
table_names = set(re.findall(r"(?m)^\| `/kmp:([a-z]+)` \|", readme))
if table_names != ids:
    fail(f"README workflow table differs: {sorted(table_names)}")
if "For you — ten commands" not in readme:
    fail("README does not state the ten-command contract")

# The seeded guide replaced the retired public demo surface.
retired_demo_assets = [
    PLUGIN / "skills" / "kmp-demo" / "SKILL.md",
    PLUGIN / "scripts" / "kmp-demo.sh",
    PLUGIN / "claude" / "commands" / "demo.md",
    PLUGIN / "demo" / "README.md",
    PLUGIN / "demo" / "checkout-latency.jsonl",
    ROOT / "crates" / "kmp-adapter-embedded" / "tests" / "demo_bundle.rs",
    ROOT / "docs" / "assets" / "kmp-demo.gif",
]
for asset in retired_demo_assets:
    if asset.exists():
        fail(f"retired user demo asset returned: {asset.relative_to(ROOT)}")

root_readme = (ROOT / "README.md").read_text(encoding="utf-8")
if "docs/assets/kmp-demo.gif" in root_readme:
    fail("root README still embeds the retired demo GIF")
if "docs/assets/campaign" in root_readme or "kmp-agent-loom.gif" in root_readme:
    fail("root README still promotes an archived, unapproved campaign asset")

# Public setup docs must agree on ownership. A bare `--codex` installer was
# the former global-wiring path; prescribing it beside the native plugin
# recreates the duplicate MCP owner that setup now refuses.
ownership_docs = [
    ROOT / "README.md",
    PLUGIN / "README.md",
    ROOT / "docs" / "embedded" / "README.md",
]
for asset in ownership_docs:
    text = asset.read_text(encoding="utf-8")
    if "codex plugin add kmp@underpass" not in text:
        fail(f"Codex native-plugin setup is missing: {asset.relative_to(ROOT)}")
    if "install-kmp-plugin.sh" in text or "--standalone" in text:
        fail(f"Codex docs prescribe retired global wiring: {asset.relative_to(ROOT)}")

if (ROOT / "scripts/mcp/install-kmp-plugin.sh").exists():
    fail("retired standalone installer still duplicates native plugin ownership")

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

retired = re.compile(
    r"\bkernel_(?:ingest|write_memory|wake|ask|goto|near|rewind|forward|trace|inspect)\b"
)
living = [
    PLUGIN / "README.md",
    *(PLUGIN / "claude" / "commands").glob("*.md"),
    *(PLUGIN / "skills").glob("*/SKILL.md"),
]
for asset in living:
    if retired.search(asset.read_text(encoding="utf-8")):
        fail(f"living instruction uses a retired tool name: {asset.relative_to(ROOT)}")

print(
    f"KMP capability contract passed: {len(tools)} MCP tools, "
    f"{len(workflows)} workflows, {len(native_skills)} native skills"
)
