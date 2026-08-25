#!/usr/bin/env python3
"""Pin temporal routing and bounded semantic language fallback for agents."""

from __future__ import annotations

import datetime as dt
import json
import pathlib
from zoneinfo import ZoneInfo


ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURE = json.loads(
    (ROOT / "tests/plugin/kmp-agent-routing.json").read_text(encoding="utf-8")
)


def fail(message: str) -> None:
    raise SystemExit(f"KMP agent routing contract: {message}")


def utc_text(value: dt.datetime) -> str:
    return value.astimezone(dt.timezone.utc).isoformat(timespec="seconds").replace(
        "+00:00", "Z"
    )


zone = ZoneInfo(FIXTURE["timezone"])
local = FIXTURE["local_interval"]
actual_interval = {
    bound: utc_text(dt.datetime.fromisoformat(local[bound]).replace(tzinfo=zone))
    for bound in ("start", "end")
}
if actual_interval != FIXTURE["expected_utc_interval"]:
    fail(f"timezone conversion differs: {actual_interval!r}")

start = dt.datetime.fromisoformat(actual_interval["start"].replace("Z", "+00:00"))
end = dt.datetime.fromisoformat(actual_interval["end"].replace("Z", "+00:00"))
material_refs: list[str] = []
excluded_refs = set(FIXTURE["excluded_refs"])
for entry in FIXTURE["temporal_boundary"]["entries"]:
    when = dt.datetime.fromisoformat(entry["timestamp"].replace("Z", "+00:00"))
    if when == start:
        material_refs.append(entry["ref"])
expected_trace: list[str] = [f"kmp_goto:{actual_interval['start']}"]
cursor = actual_interval["start"]
for page in FIXTURE["temporal_pages"]:
    expected_trace.append(f"kmp_forward:{cursor}")
    for entry in page["entries"]:
        when = dt.datetime.fromisoformat(entry["timestamp"].replace("Z", "+00:00"))
        if when == start:
            fail("strictly-after forward fixture incorrectly returned the start boundary")
        if start <= when < end:
            material_refs.append(entry["ref"])
    cursor = page["next_cursor"]
    if cursor is None:
        break

if cursor is not None:
    fail(f"fixture stopped with an unconsumed continuation cursor: {cursor}")
if len(expected_trace) != len(FIXTURE["temporal_pages"]) + 1:
    fail("not every temporal page was consumed after the boundary probe")
material_refs = list(dict.fromkeys(material_refs))
if excluded_refs.intersection(material_refs):
    fail("an out-of-window boundary ref entered the material result")

case_refs = []
for case in FIXTURE["temporal_cases"]:
    if any("kmp_ask" in call for call in case["tool_trace"]):
        fail(f"temporal {case['language']} case entered semantic Ask")
    if case["tool_trace"] != expected_trace:
        fail(f"temporal {case['language']} case did not consume every cursor")
    if case["material_refs"] != material_refs:
        fail(f"temporal {case['language']} case selected different interval refs")
    case_refs.append(case["material_refs"])
if any(refs != case_refs[0] for refs in case_refs[1:]):
    fail("English and Spanish temporal prompts selected different material refs")

semantic = FIXTURE["semantic_case"]
fallbacks = semantic["fallback_languages"]
if semantic["primary_result"] != "UNKNOWN":
    fail("semantic fallback ran before the primary user-language UNKNOWN")
if len(fallbacks) != len(set(fallbacks)) or len(fallbacks) > 3:
    fail("semantic fallback is duplicate or unbounded")
if len(semantic["translated_queries"]) != len(fallbacks):
    fail("semantic fallback does not have exactly one translated query per language")
expected_semantic_trace = [f"kmp_ask:{semantic['user_language']}"] + [
    f"kmp_ask:{language}" for language in fallbacks
]
if semantic["tool_trace"] != expected_semantic_trace:
    fail("semantic Ask retries are not primary-first and bounded")
if semantic["answer_language"] != semantic["user_language"]:
    fail("semantic answer language differs from the user's language")

stored = semantic["stored_evidence_json"].encode("utf-8")
returned = semantic["fallback_returned_evidence_json"].encode("utf-8")
if returned != stored:
    fail("fallback translated or rewrote stored evidence bytes")
evidence = json.loads(stored)
for field in ("ref", "text", "relation", "source"):
    if field not in evidence:
        fail(f"semantic evidence fixture lost {field}")
for field in ("why", "evidence"):
    if field not in evidence["relation"]:
        fail(f"semantic relation fixture lost {field}")

instruction_assets = [
    ROOT / "plugins/kmp/skills/kmp-memory/SKILL.md",
    ROOT / "plugins/kmp/codex/AGENTS.kmp.md",
    ROOT / "crates/kmp-mcp/src/agent_policy.rs",
]
required = (
    "temporal intent",
    "half-open UTC interval",
    "translate only the query",
    "user's language",
    "kmp_goto",
    "deduplicate",
    "UNKNOWN",
)
for asset in instruction_assets:
    text = asset.read_text(encoding="utf-8").casefold()
    missing = [phrase for phrase in required if phrase.casefold() not in text]
    if missing:
        fail(f"{asset.relative_to(ROOT)} lacks routing clauses: {missing}")

write_instruction_assets = [
    ROOT / "plugins/kmp/skills/kmp-memory/SKILL.md",
    ROOT / "plugins/kmp/codex/AGENTS.kmp.md",
    ROOT / "plugins/kmp/codex/prompts/kmp-moves.md",
    ROOT / "plugins/kmp/claude/commands/moves.md",
]
for asset in write_instruction_assets:
    text = asset.read_text(encoding="utf-8").casefold()
    if "dry_run=false" not in text:
        fail(f"{asset.relative_to(ROOT)} does not select commit as the default")
    if "one call" not in text and "single-call" not in text:
        fail(f"{asset.relative_to(ROOT)} reintroduced a two-call writer workflow")
    if "dry_run=true" not in text or "preview" not in text:
        fail(f"{asset.relative_to(ROOT)} lost the explicit preview path")
    if ("invalid" not in text and "validation fail" not in text) or "nothing" not in text:
        fail(f"{asset.relative_to(ROOT)} does not state fail-before-write behavior")

protocol = (ROOT / "crates/kmp-mcp/src/protocol.rs").read_text(encoding="utf-8")
if "Normal writes are one call: omit `options.dry_run` or set it to false" not in protocol:
    fail("live MCP schema does not document the dry_run=false write default")
if "Set it to true only for an explicitly requested preview" not in protocol:
    fail("live MCP schema lost the explicit dry-run preview path")

print(
    "KMP agent routing contract passed: two languages, complete temporal pages, "
    "bounded semantic fallback, byte-exact evidence, single-call validated writes"
)
