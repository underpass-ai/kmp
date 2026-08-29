#!/usr/bin/env python3
"""Derive monotonic audio/edit anchors from retained product evidence."""

from __future__ import annotations

import hashlib
import json
import math
import pathlib
import sys


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def digest(path: pathlib.Path) -> str:
    return digest_bytes(path.read_bytes())


def jsonl(path: pathlib.Path) -> list[dict]:
    rows = []
    for line in path.read_bytes().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        row["_line_sha256"] = digest_bytes(line)
        rows.append(row)
    return rows


if len(sys.argv) != 3:
    raise SystemExit("usage: build-anchors.py RUN_DIR SCENARIO")
run = pathlib.Path(sys.argv[1]).resolve()
scenario = json.loads(pathlib.Path(sys.argv[2]).read_text())
video_id = scenario["id"]
wire = jsonl(run / "tool-calls.jsonl")
terminal = jsonl(run / "terminal-events.jsonl")
revisions = jsonl(run / "viewer-revisions.jsonl")
lifecycle_path = run / "process-lifecycle.json"
lifecycle = json.loads(lifecycle_path.read_text())
obs_wire = jsonl(run / "obs-websocket.jsonl")
obs_schedule = jsonl(run / "obs-scene-schedule.jsonl")

requests: dict[tuple[str, object], dict] = {}
transactions: list[dict] = []
for row in wire:
    message = row.get("message", {})
    key = (row.get("process_id", "process"), message.get("id"))
    if row.get("direction") == "client_to_server" and message.get("method") == "tools/call":
        requests[key] = row
    elif row.get("direction") == "server_to_client" and key in requests:
        transactions.append({"request": requests[key], "response": row})


def tool_response(name: str, process_id: str | None = None, occurrence: int = 0) -> dict:
    matches = [
        item for item in transactions
        if item["request"]["message"].get("params", {}).get("name") == name
        and (process_id is None or item["request"].get("process_id") == process_id)
    ]
    if occurrence >= len(matches):
        raise KeyError(f"missing tool response {process_id or '*'}:{name}[{occurrence}]")
    return matches[occurrence]


def terminal_event(text: str) -> dict:
    return next(row for row in terminal if row.get("text") == text)


def revision(explanation: str) -> dict:
    return next(row for row in revisions if row.get("explanation") == explanation)


def process_run(process_id: str) -> dict:
    return next(row for row in lifecycle.get("process_runs", []) if row.get("process_id") == process_id)


started_event = next(
    row for row in obs_wire
    if row.get("op") == 5
    and row.get("event", {}).get("eventType") == "RecordStateChanged"
    and row.get("event", {}).get("eventData", {}).get("outputState") == "OBS_WEBSOCKET_OUTPUT_STARTED"
)
record_start_ns = int(started_event["monotonic_ns"])
stop = json.loads((run / "obs-stop.json").read_text())
record_stop_ns = int(stop["monotonic_ns"])
probe = json.loads((run / "ffprobe.json").read_text())
duration_ns = round(float(probe["format"]["duration"]) * 1_000_000_000)
clock_map = {
    "contract": "kmp.capture.monotonic-frame-map.v1",
    "video_id": video_id,
    "timebase": "CLOCK_MONOTONIC nanoseconds",
    "video_timebase": "nanoseconds from first encoded frame PTS",
    "points": [
        {
            "monotonic_ns": str(record_start_ns),
            "video_pts_ns": "0",
            "source": "OBS RecordStateChanged/STARTED receipt",
            "evidence": {"obs_websocket_line_sha256": started_event["_line_sha256"]},
        },
        {
            "monotonic_ns": str(record_stop_ns),
            "video_pts_ns": str(duration_ns),
            "source": "OBS StopRecord response + ffprobe duration",
            "evidence": {"obs_stop_sha256": digest(run / "obs-stop.json"), "ffprobe_sha256": digest(run / "ffprobe.json")},
        },
    ],
    "mapping": "piecewise_linear_between_points",
    "monotonic": record_stop_ns > record_start_ns and duration_ns > 0,
}
(run / "clock-map.json").write_text(json.dumps(clock_map, indent=2) + "\n")


def pts_ns(monotonic_ns: int) -> int:
    if monotonic_ns <= record_start_ns:
        return 0
    if monotonic_ns >= record_stop_ns:
        return duration_ns
    return round((monotonic_ns - record_start_ns) * duration_ns / (record_stop_ns - record_start_ns))


anchors: list[dict] = []


def add(anchor: str, monotonic_ns: int, source: str, evidence: dict) -> None:
    anchors.append({
        "video_id": video_id,
        "anchor": anchor,
        "monotonic_ns": str(monotonic_ns),
        "video_pts_ns": str(pts_ns(monotonic_ns)),
        "source": source,
        "evidence": evidence,
    })


def add_tool(anchor: str, name: str, process_id: str, occurrence: int = 0) -> None:
    item = tool_response(name, process_id, occurrence)
    row = item["response"]
    add(anchor, int(row["monotonic_ns"]), "tool-calls.jsonl", {
        "process_id": process_id,
        "rpc_id": row["message"].get("id"),
        "wire_sha256": row["wire_sha256"],
        "evidence_line_sha256": row["_line_sha256"],
    })


def add_terminal(anchor: str, text: str) -> None:
    row = terminal_event(text)
    add(anchor, int(row["monotonic_ns"]), "terminal-events.jsonl", {"terminal_event_line_sha256": row["_line_sha256"]})


def add_revision(anchor: str, explanation: str) -> None:
    row = revision(explanation)
    add(anchor, int(row["monotonic_ns"]), "viewer-revisions.jsonl", {
        "view_revision": row.get("view_revision"),
        "body_sha256": row.get("body_sha256"),
        "origin": row.get("origin"),
        "evidence_line_sha256": row["_line_sha256"],
    })


def add_scene(anchor: str, scene: str, occurrence: int = 0) -> None:
    matches = [row for row in obs_schedule if row.get("scene") == scene]
    if occurrence >= len(matches):
        raise KeyError(f"missing OBS scene {scene}[{occurrence}]")
    row = matches[occurrence]
    add(anchor, int(row["response_monotonic_ns"]), "obs-scene-schedule.jsonl", {
        "scene": scene,
        "requested_at_ms": row.get("requested_at_ms"),
        "lateness_ms": row.get("lateness_ms"),
        "evidence_line_sha256": row["_line_sha256"],
    })


if video_id == "fresh-process-same-why":
    add_terminal("hook_visible", "End the session. Keep the why.")
    add_tool("memory_write_committed", "kmp_write_memory", "session-01")
    add_revision("viewer_decision_visible", "show the committed decision and its evidence")
    add_revision("viewer_evidence_visible", "show the committed decision and its evidence")
    first = process_run("session-01")
    add("process_1_exit", int(first["end"]["monotonic_ns"]), "process-lifecycle.json", {
        "lifecycle_sha256": digest(lifecycle_path), "pid": first["pid"], "exit": first["exit"], "store_fingerprint": first["store_fingerprint"],
    })
    second = process_run("session-02")
    add("process_2_spawn", int(second["start"]["monotonic_ns"]), "process-lifecycle.json", {
        "lifecycle_sha256": digest(lifecycle_path), "pid": second["pid"], "store_fingerprint": second["store_fingerprint"],
    })
    add_terminal("question_2_visible", "Why does KMP Embedded use SQLite WAL?")
    add_revision("recovered_decision_visible", "recover the decision in a fresh process")
    add_revision("recovered_evidence_visible", "recover the decision in a fresh process")
    add_terminal("end_card_visible", "Fresh process. Same decision. Evidence attached. Run KMP Embedded → github.com/underpass-ai/kmp")
elif video_id == "two-processes-one-memory":
    add_terminal("hook_visible", "Process A writes it. Process B recovers the why.")
    second = process_run("process-b")
    add("process_b_spawn", int(second["start"]["monotonic_ns"]), "process-lifecycle.json", {
        "lifecycle_sha256": digest(lifecycle_path), "pid": second["pid"], "store_fingerprint": second["store_fingerprint"],
    })
    add_tool("process_a_write_committed", "kmp_write_memory", "process-a")
    add_revision("process_a_decision_visible", "show the memory Process A committed")
    add_revision("viewer_decision_visible", "show the memory Process A committed")
    add_revision("viewer_evidence_visible", "show the memory Process A committed")
    add_terminal("process_b_question_visible", "Why is max_connections back at 200?")
    add_tool("shared_store_recall_visible", "kmp_inspect", "process-b")
    add_revision("process_b_recovered_view_visible", "recover why max_connections returned to 200")
    add_revision("recovered_decision_visible", "recover why max_connections returned to 200")
    add_revision("recovered_evidence_visible", "recover why max_connections returned to 200")
    add_terminal("end_card_visible", "Two processes. One local memory. See KMP Embedded → github.com/underpass-ai/kmp")
elif video_id == "keep-the-wrong-turn":
    add_terminal("hook_visible", "Delete the wrong turn. Lose the why.")
    add_terminal("selection_question_visible", "Show me the memory behind the pool-limit decision.")
    add_revision("selection_applied_revision", "show the memory behind the pool-limit decision")
    add_revision("clock_prism_visible", "show the memory behind the pool-limit decision")
    add_terminal("trace_question_visible", "Great! Can you light up the proof path?")
    add_revision("trace_applied_revision", "light up the proof path")
    for index, relation in enumerate(("verified_by", "supersedes", "chosen_because", "depends_on"), start=1):
        add_revision(f"hop_{index}_focus", f"proof hop {index} — {relation}")
    # The complete path is already a real ChronoLoom state. This anchor marks
    # when OBS makes that stable state the primary picture after the four hops.
    add_scene("full_path_visible", "KMP/ChronoFocus", 1)
    add_terminal("nice_visible", "Nice.")
    add_terminal("signature_visible", "Memory, with receipts. See KMP Embedded → github.com/underpass-ai/kmp")
    add_terminal("end_card_visible", "Memory, with receipts. See KMP Embedded → github.com/underpass-ai/kmp")
else:
    # Technical smoke uses only generic closure anchors.
    add_terminal("recording_clock_locked", "OBS RECORDING · evidence clock locked")

anchors.sort(key=lambda item: (int(item["monotonic_ns"]), item["anchor"]))
anchor_path = run / "anchors.jsonl"
anchor_path.write_text("".join(json.dumps(item, sort_keys=True) + "\n" for item in anchors))
manifest = {
    "contract": "kmp.capture.anchor-set.v1",
    "video_id": video_id,
    "ordered": all(int(before["monotonic_ns"]) <= int(after["monotonic_ns"]) for before, after in zip(anchors, anchors[1:])),
    "count": len(anchors),
    "anchors_sha256": digest(anchor_path),
    "clock_map_sha256": digest(run / "clock-map.json"),
}
(run / "anchors-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

# Resolve the creative plan against the observed picture clock. A nominal cue
# is never evidence that the corresponding product state was already visible;
# the next encoded 30 fps frame at or after the evidence anchor is.
audio_contract_path = run / "audio-contract.json"
audio_contract = json.loads(audio_contract_path.read_text())
audio_master = audio_contract.get("masters", {}).get(video_id)
if audio_master:
    by_name = {item["anchor"]: item for item in anchors}
    resolved = []
    for cue in audio_master.get("cue_anchors", []):
        visible = by_name[cue["visible_anchor"]]
        visible_seconds = int(visible["video_pts_ns"]) / 1_000_000_000
        first_safe_frame = math.ceil(visible_seconds * 30) / 30
        resolved_at = max(float(cue["at"]), float(cue.get("not_before", 0)), first_safe_frame)
        resolved.append({
            **cue,
            "planned_at": float(cue["at"]),
            "resolved_at": round(resolved_at, 9),
            "visible_anchor_pts": visible_seconds,
            "visible_anchor_evidence": visible["evidence"],
        })
    cue_resolution = {
        "contract": "kmp.capture.audio-cue-resolution.v1",
        "video_id": video_id,
        "frame_rate": "30/1",
        "policy": "max(planned_at, not_before, first encoded frame at or after visible anchor)",
        "audio_contract_sha256": digest(audio_contract_path),
        "anchors_sha256": digest(anchor_path),
        "cues": resolved,
    }
    (run / "audio-cues.json").write_text(json.dumps(cue_resolution, indent=2) + "\n")
print(f"anchors: {len(anchors)} ordered events for {video_id}")
