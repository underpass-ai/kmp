#!/usr/bin/env python3
"""Pin temporal, semantic, audit, and shared-view routing for agents."""

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

temporal_moves = {"kmp_goto", "kmp_near", "kmp_rewind", "kmp_forward"}
cases = {case["original_goal_class"]: case for case in FIXTURE["result_routing_cases"]}
for goal_class in ("current_state", "release_history"):
    case = cases[goal_class]
    if not case["ask_results"] or any(result != "UNKNOWN" for result in case["ask_results"]):
        fail(f"{goal_class} fixture did not exhaust unanswered semantic retries")
    if case["next_move"] not in temporal_moves or case["terminal"]:
        fail(f"{goal_class} UNKNOWN did not reclassify into temporal navigation")

semantic_unknown = cases["pure_semantics"]
if (
    any(result != "UNKNOWN" for result in semantic_unknown["ask_results"])
    or semantic_unknown["next_move"] != "return_UNKNOWN"
    or not semantic_unknown["terminal"]
):
    fail("a genuinely semantic UNKNOWN did not remain a valid terminal result")

if cases["consequential_claim"]["next_move"] != "kmp_inspect":
    fail("a consequential claim did not route from retrieval to inspection")
if cases["connection_claim"]["next_move"] != "kmp_trace":
    fail("a claimed connection did not route from retrieval to trace")

partial = FIXTURE["partial_recall_case"]
if partial["has_more"] and partial["goal_depends_on_omitted_material"]:
    continuation = f"kmp_wake:{partial['next_cursor']}"
    try:
        continuation_index = partial["tool_trace"].index(continuation)
        repository_index = partial["tool_trace"].index("repository_read")
    except ValueError as error:
        fail(f"partial recall fixture lost its continuation boundary: {error}")
    if continuation_index >= repository_index:
        fail("repository evidence was read before the relevant KMP page completed")

opaque = FIXTURE["opaque_ref_case"]
passed_refs = [opaque["tool_arguments"]["from_ref"], opaque["tool_arguments"]["to_ref"]]
if passed_refs != opaque["returned_refs"]:
    fail("an opaque ref changed between retrieval and the next KMP call")
if any(ref.startswith(f"{opaque['about']}:") for ref in passed_refs):
    fail("an opaque ref was prefixed with its about")

opaque_about = FIXTURE["opaque_about_case"]
supplied_about = opaque_about["supplied_about"]
passed_abouts = [call["about"] for call in opaque_about["tool_arguments"]]
if not passed_abouts or any(about != supplied_about for about in passed_abouts):
    fail("an opaque about changed between the user input and a KMP call")
if set(passed_abouts).intersection(opaque_about["forbidden_variants"]):
    fail("an opaque about was stripped or prefixed")

unknown_guard = FIXTURE["semantic_unknown_guard_case"]
# The question is asked once in the kernel's search language with the user's
# own words as asked_as, then at most once in the user's own words.
expected_languages = [unknown_guard["search_language"], unknown_guard["user_language"]]
selections = unknown_guard["ask_selections"]
actual_languages = [selection["language"] for selection in selections]
if actual_languages != expected_languages or len(actual_languages) != len(set(actual_languages)):
    fail("semantic Ask did not ask once in the search language and once in the user's words")
if selections[0].get("asked_as") != unknown_guard["user_language"]:
    fail("the English selection did not carry the user's own words as asked_as")
if any(selection["about"] != unknown_guard["about"] for selection in selections):
    fail("semantic Ask changed the opaque about during language fallback")
if any(selection["selection"] != "initial" for selection in selections):
    fail("semantic Ask fixture contains an unbounded same-language restart")
if unknown_guard["same_language_restarts"]:
    fail("semantic Ask restarted in a language it had already selected")
continuation = unknown_guard["page_continuation"]
if (
    not continuation["cursor"]
    or not continuation["bound_arguments_unchanged"]
    or continuation["counts_as_new_selection"]
):
    fail("Ask cursor pagination is not distinguished from a new selection")
if (
    not unknown_guard["ask_results"]
    or any(result != "UNKNOWN" for result in unknown_guard["ask_results"])
    or unknown_guard["next_move"] != "return_UNKNOWN"
):
    fail("bounded semantic UNKNOWN did not terminate")
if set(unknown_guard["forbidden_next_moves"]).intersection(
    unknown_guard["post_unknown_tool_trace"]
):
    fail("bounded semantic UNKNOWN bypassed Ask through the graph")
if unknown_guard["post_unknown_tool_trace"] != [unknown_guard["next_move"]]:
    fail("bounded semantic UNKNOWN made another tool call before terminating")

# Nothing above starts a route: KMP is entered when it is asked for. A session
# that never mentions memory must reach the end of its work having called
# nothing, and the fixture proves both directions of that gate.
SELECTION_SIGNALS = {
    "user_named_kmp",
    "kmp_skill_invoked",
    "project_instructions_opt_in",
}
routing_modes = set()
unprompted = 0
for case in FIXTURE["invocation_gate_cases"]:
    name = case["name"]
    routing = case["memory_routing"]
    if routing not in {"on_request", "always"}:
        fail(f"{name}: unknown memory routing mode {routing!r}")
    routing_modes.add(routing)
    signals = set(case["selection_signals"])
    if signals - SELECTION_SIGNALS:
        fail(f"{name}: unknown selection signal {sorted(signals - SELECTION_SIGNALS)}")
    selected = bool(signals) or routing == "always"
    called = [call for call in case["tool_trace"] if call.startswith("kmp_")]
    if selected and not called:
        fail(f"{name}: a selected route never entered KMP")
    if not selected and called:
        fail(f"{name}: KMP was called without being asked: {called}")
    if not selected:
        unprompted += 1
if routing_modes != {"on_request", "always"}:
    fail("the invocation gate does not cover both routing modes")
if not unprompted:
    fail("no fixture proves an unprompted session stays out of KMP")
for signal in SELECTION_SIGNALS:
    if not any(
        signal in case["selection_signals"] for case in FIXTURE["invocation_gate_cases"]
    ):
        fail(f"the invocation gate does not cover the {signal} signal")

instruction_assets = [
    ROOT / "plugins/kmp/skills/kmp-memory/SKILL.md",
    ROOT / "crates/kmp-mcp/src/agent_policy/instructions.rs",
]
required = (
    "temporal intent",
    "half-open UTC interval",
    "asked_as",
    "user's language",
    "kmp_goto",
    "deduplicate",
    "UNKNOWN",
    "reclassify the original goal",
    "repository evidence",
    "consequential claim",
    "refs are opaque identifiers",
    "never prefix or qualify",
    "instead of guessing",
)
for asset in instruction_assets:
    text = asset.read_text(encoding="utf-8").casefold()
    missing = [phrase for phrase in required if phrase.casefold() not in text]
    if missing:
        fail(f"{asset.relative_to(ROOT)} lacks routing clauses: {missing}")

opaque_ref_instruction_assets = instruction_assets
for asset in opaque_ref_instruction_assets:
    text = " ".join(asset.read_text(encoding="utf-8").casefold().split())
    for phrase in (
        "refs are opaque identifiers",
        "never prefix or qualify",
        "instead of guessing",
    ):
        if phrase not in text:
            fail(f"{asset.relative_to(ROOT)} lost opaque-ref rule: {phrase}")

for asset in opaque_ref_instruction_assets:
    text = " ".join(asset.read_text(encoding="utf-8").casefold().split())
    for phrase in (
        "abouts are opaque routing identifiers",
        "never strip or add a kind prefix",
        "byte-for-byte into every",
        "at most once in the user's own words",
        "does not authorize another selection",
        "projection.page.next_cursor",
        "inspect the about/root",
    ):
        if phrase not in text:
            fail(f"{asset.relative_to(ROOT)} lost bounded-Ask/about rule: {phrase}")

for asset in instruction_assets:
    text = " ".join(asset.read_text(encoding="utf-8").casefold().split())
    for phrase in ("opt-in", "make no kmp call", "always-on"):
        if phrase not in text:
            fail(f"{asset.relative_to(ROOT)} lost the invocation gate: {phrase}")
    for phrase in ("use whenever", "always enters through"):
        if phrase in text:
            fail(f"{asset.relative_to(ROOT)} makes KMP mandatory again: {phrase}")

write_instruction_assets = [
    ROOT / "plugins/kmp/skills/kmp-memory/SKILL.md",
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

# The inference-prompt fixtures teach a model the writer directly, and they
# drifted back to the two-call workflow while every gated surface said one
# call. They are gated the same way now.
writer_prompt_assets = [
    ROOT / "api/examples/inference-prompts/kmp-write-memory.txt",
    ROOT / "api/examples/inference-prompts/kmp-write-memory.request.json",
]
for asset in writer_prompt_assets:
    text = asset.read_text(encoding="utf-8").casefold()
    if "dry_run=true` first" in text or "use options.dry_run=true and" in text:
        fail(f"{asset.relative_to(ROOT)} tells the writer to preview before committing")
    if "set it to false" not in text or "one call" not in text:
        fail(f"{asset.relative_to(ROOT)} does not select commit as the default")
    if "summary_en" not in text:
        fail(f"{asset.relative_to(ROOT)} does not ask the writer for the English search summary")

memory_skill = (ROOT / "plugins/kmp/skills/kmp-memory/SKILL.md").read_text(
    encoding="utf-8"
)
memory_skill_folded = " ".join(memory_skill.casefold().split())
for tool in ("kmp_view_open", "kmp_view_apply_intent", "kmp_view_get_state"):
    if tool not in memory_skill:
        fail(f"kmp-memory does not route the shared view through {tool}")
for phrase in (
    "show me the memory behind this decision",
    "muéstrame",
    "enséñame",
    "expected_revision",
    "view is not itself proof",
    "do not call `kmp_view_open` again",
):
    if phrase.casefold() not in memory_skill_folded:
        fail(f"kmp-memory lost shared-view routing clause: {phrase}")

public_readme = (ROOT / "README.md").read_text(encoding="utf-8").casefold()
invitation = "show me the memory behind this decision"
if invitation not in public_readme or invitation not in memory_skill.casefold():
    fail("the public ChronoLoom invitation is not routed by kmp-memory")

# Read from the reviewed surface rather than the Rust that builds it. What an
# agent is routed by is the description a host serves, and `tools_list.json` is
# that document, pinned against the running binary by `tool_surface_parity`.
surface = json.loads(
    (ROOT / "crates/kmp-mcp/fixtures/contract/tools_list.json").read_text(encoding="utf-8")
)
writer = next(
    (tool for tool in surface["tools"] if tool["name"] == "kmp_write_memory"), None
)
if writer is None:
    fail("the advertised surface no longer offers kmp_write_memory")
else:
    description = writer["description"]
    if "Normal writes are one call: omit `options.dry_run` or set it to false" not in description:
        fail("live MCP schema does not document the dry_run=false write default")
    if "Set it to true only for an explicitly requested preview" not in description:
        fail("live MCP schema lost the explicit dry-run preview path")

print(
    "KMP agent routing contract passed: opt-in invocation, two languages, "
    "complete temporal pages, "
    "result-driven UNKNOWN routing, bounded Ask selections, audit gates, opaque "
    "abouts and refs, byte-exact evidence, shared-view intents, single-call "
    "validated writes"
)
