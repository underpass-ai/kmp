#!/usr/bin/env python3
from __future__ import annotations

import configparser
import hashlib
import json
import pathlib
import re
import subprocess
import sys

CAMPAIGN = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(CAMPAIGN / "scripts"))

from capture_contract import credential_findings


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def json_lines(path: pathlib.Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


if len(sys.argv) != 4:
    raise SystemExit("usage: verify-run.py RUN_DIR SCENARIO EDL")
run = pathlib.Path(sys.argv[1]).resolve()
scenario = json.loads(pathlib.Path(sys.argv[2]).read_text())
edl = json.loads(pathlib.Path(sys.argv[3]).read_text())

checks: list[dict] = []


def check(name: str, condition: bool, evidence: object) -> None:
    checks.append({"name": name, "passed": bool(condition), "evidence": evidence})


global_ini = run / "obs-config" / "obs-studio" / "global.ini"
global_text = global_ini.read_text()
check("obs_password_redacted", "ServerPassword=<ephemeral-redacted>" in global_text, str(global_ini))

check("ephemeral_capabilities_removed", not any(run.rglob("*.private")), "no *.private files")
credential_audit = credential_findings(run)
check(
    "no_ephemeral_credentials",
    not credential_audit,
    credential_audit or "credential-shaped fields are absent or explicitly redacted",
)

recording = run / "obs-recording.mkv"
probe = subprocess.run(
    ["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(recording)],
    check=True,
    text=True,
    capture_output=True,
)
probe_data = json.loads(probe.stdout)
(run / "ffprobe.json").write_text(json.dumps(probe_data, indent=2) + "\n")
video = next((stream for stream in probe_data["streams"] if stream["codec_type"] == "video"), None)
picture_fps = float(edl["picture_contract"]["canvas"]["fps"])
duration_tolerance = 1 / picture_fps + 0.001
rate_parts = (video or {}).get("r_frame_rate", "0/1").split("/")
observed_fps = float(rate_parts[0]) / float(rate_parts[1]) if len(rate_parts) == 2 and float(rate_parts[1]) else 0
check("obs_raw_exists", recording.is_file() and recording.stat().st_size > 0, recording.stat().st_size)
check("video_codec_h264", video and video["codec_name"] == "h264", video and video["codec_name"])
check("video_zero_latency_encoder", video and int(video.get("has_b_frames", -1)) == 0, video and {"has_b_frames": video.get("has_b_frames")})
check("video_resolution", video and video["width"] == 1920 and video["height"] == 1080, video and [video["width"], video["height"]])
check("video_rate", video and observed_fps == picture_fps, video and video.get("r_frame_rate"))
duration = float(probe_data["format"].get("duration", 0))
check("duration_covers_scenario", duration >= scenario["duration_ms"] / 1000 - duration_tolerance, duration)
check(
    "duration_within_one_frame",
    abs(duration - scenario["duration_ms"] / 1000) <= duration_tolerance,
    {"actual": duration, "target": scenario["duration_ms"] / 1000, "tolerance_seconds": duration_tolerance},
)

wire = json_lines(run / "tool-calls.jsonl")
tool_requests = [
    row for row in wire
    if row["direction"] == "client_to_server"
    and row["message"].get("method") == "tools/call"
]
check("real_mcp_wire", len(tool_requests) >= len(scenario["bootstrap"]) + sum(step["type"] == "tool" for step in scenario["steps"]), len(tool_requests))
check("wire_hashes_present", all(re.fullmatch(r"[0-9a-f]{64}", row["wire_sha256"]) for row in wire), len(wire))

lifecycle = json.loads((run / "process-lifecycle.json").read_text())
declared = scenario.get("processes") or [
    {"id": "process", "store_id": (scenario.get("stores") or [{"id": "default"}])[0]["id"], "autostart": True, "browser": True}
]
process_runs = lifecycle.get("process_runs")
if process_runs is None:
    process_runs = [{
        "process_id": declared[0]["id"],
        "store_id": declared[0]["store_id"],
        "pid": lifecycle.get("mcp_pid"),
        "exit": lifecycle.get("mcp_exit"),
    }]
declared_ids = {item["id"] for item in declared}
started_ids = {item.get("process_id") for item in process_runs}
check("all_declared_processes_started", declared_ids <= started_ids, {"declared": sorted(declared_ids), "started": sorted(started_ids)})
process_pids = [item.get("pid") for item in process_runs]
check("mcp_pids_distinct", all(isinstance(pid, int) for pid in process_pids) and len(process_pids) == len(set(process_pids)), process_pids)
check(
    "mcp_clean_exit",
    lifecycle.get("failure") is None
    and bool(process_runs)
    and all(item.get("exit", {}).get("code") == 0 for item in process_runs),
    [{"process_id": item.get("process_id"), "instance": item.get("instance"), "exit": item.get("exit")} for item in process_runs],
)
declared_store = {item["id"]: item["store_id"] for item in declared}
check(
    "process_store_binding",
    all(item.get("store_id") == declared_store.get(item.get("process_id")) for item in process_runs),
    [{"process_id": item.get("process_id"), "store_id": item.get("store_id"), "fingerprint": item.get("store_fingerprint")} for item in process_runs],
)
if scenario["id"] == "fresh-process-same-why":
    first = next((item for item in process_runs if item.get("process_id") == "session-01"), {})
    second = next((item for item in process_runs if item.get("process_id") == "session-02"), {})
    first_end = int(first.get("end", {}).get("monotonic_ns", "0"))
    second_start = int(second.get("start", {}).get("monotonic_ns", "0"))
    check("fresh_process_handoff_order", first_end > 0 and second_start > first_end, {"session_01_end": first_end, "session_02_start": second_start})
if scenario["id"] == "two-processes-one-memory":
    first = next((item for item in process_runs if item.get("process_id") == "process-a"), {})
    second = next((item for item in process_runs if item.get("process_id") == "process-b"), {})
    starts = [int(first.get("start", {}).get("monotonic_ns", "0")), int(second.get("start", {}).get("monotonic_ns", "0"))]
    ends = [int(first.get("end", {}).get("monotonic_ns", "0")), int(second.get("end", {}).get("monotonic_ns", "0"))]
    check("two_processes_overlap", all(starts) and all(ends) and max(starts) < min(ends), {"starts": starts, "ends": ends})
check("binary_bound", re.fullmatch(r"[0-9a-f]{64}", lifecycle["binary"]["sha256"]) is not None, lifecycle["binary"])
repository = lifecycle.get("repository", {})
check(
    "worktree_state_disclosed",
    isinstance(repository.get("worktree_dirty"), bool)
    and isinstance(repository.get("changed_paths"), list)
    and re.fullmatch(r"[0-9a-f]{64}", repository.get("changed_paths_sha256", "")) is not None,
    {
        "commit": repository.get("commit"),
        "worktree_dirty": repository.get("worktree_dirty"),
        "changed_path_count": len(repository.get("changed_paths", [])),
        "changed_paths_sha256": repository.get("changed_paths_sha256"),
    },
)

store = json.loads((run / "stores.json").read_text())
sqlite_files = []
store_records = store.get("stores") or [store]
for record in store_records:
    for item in record["files"]:
        file = pathlib.Path(record["selected_data_dir"]) / item["path"]
        if file.exists() and file.read_bytes()[:16] == b"SQLite format 3\x00":
            sqlite_files.append({"store_id": record.get("id", "default"), **item})
check("isolated_sqlite_store", all(item["isolated_from_user_store"] for item in store_records) and bool(sqlite_files), sqlite_files)
store_by_id = {item.get("id", "default"): item for item in store_records}
check(
    "shared_store_fingerprint",
    all(
        item.get("data_dir") == store_by_id.get(item.get("store_id"), {}).get("selected_data_dir")
        and item.get("store_fingerprint") == store_by_id.get(item.get("store_id"), {}).get("fingerprint")
        for item in process_runs
        if lifecycle.get("process_runs") is not None
    ),
    {item.get("id", "default"): item.get("fingerprint") for item in store_records},
)

forbidden_wire_tools = {"kmp_export", "kmp_import", "export", "import"}
called_tools = [row["message"].get("params", {}).get("name") for row in tool_requests]
check("no_export_or_import", not (set(called_tools) & forbidden_wire_tools), called_tools)

if scenario["id"] == "keep-the-wrong-turn":
    ingest = next((row for row in tool_requests if row["message"].get("params", {}).get("name") == "kmp_ingest"), None)
    memory = (ingest or {}).get("message", {}).get("params", {}).get("arguments", {}).get("memory", {})
    check(
        "deterministic_fixture_7_7_3",
        len(memory.get("entries", [])) == 7 and len(memory.get("relations", [])) == 7 and len(memory.get("evidence", [])) == 3,
        {"entries": len(memory.get("entries", [])), "relations": len(memory.get("relations", [])), "evidence": len(memory.get("evidence", []))},
    )
    relations = [item.get("rel") for item in memory.get("relations", [])]
    proof_order = ["verified_by", "supersedes", "chosen_because", "depends_on"]
    proof_positions = [relations.index(item) if item in relations else -1 for item in proof_order]
    check("four_hop_relations_present", all(position >= 0 for position in proof_positions), {"relations": relations, "required": proof_order})

revisions = json_lines(run / "viewer-revisions.jsonl")
check("browser_observed_view", bool(revisions), revisions[-1] if revisions else None)
check("browser_observed_long_poll", any(row.get("long_poll") and isinstance(row.get("view_revision"), int) for row in revisions), revisions)
check("browser_observed_agent_intent", any(row.get("actor") == "agent" for row in revisions), revisions)
intent_count = sum(
    row["message"].get("params", {}).get("name") == "kmp_view_apply_intent"
    for row in tool_requests
)
check(
    "browser_observed_each_intent",
    sum(row.get("actor") == "agent" for row in revisions) >= intent_count,
    {"intents": intent_count, "agent_revisions": sum(row.get("actor") == "agent" for row in revisions)},
)
browser_network = json_lines(run / "browser-network.jsonl")
browser_switches = lifecycle.get("browser_switches", [])
cdp_ready_ns = int(json.loads((run / "control" / "cdp-ready").read_text())["monotonic_ns"])
post_ready_switches = [
    item for item in browser_switches
    if int(item.get("at", {}).get("monotonic_ns", "0")) >= cdp_ready_ns
]
required_navigations = 1 + len(post_ready_switches)
check(
    "browser_switches_observed",
    sum(row.get("phase") == "viewer_navigation" for row in browser_network) >= required_navigations,
    {
        "pre_cdp_switches_collapsed_into_initial_navigation": len(browser_switches) - len(post_ready_switches),
        "post_cdp_switches": len(post_ready_switches),
        "required_navigations": required_navigations,
        "navigations": sum(row.get("phase") == "viewer_navigation" for row in browser_network),
    },
)

obs_wire = json_lines(run / "obs-websocket.jsonl")
create_input = [
    row for row in obs_wire
    if row.get("direction") == "client_to_obs"
    and row.get("d", {}).get("requestType") == "CreateInput"
]
input_requests = [row["d"]["requestData"] for row in create_input]
check(
    "obs_isolated_xshm_sources",
    len(input_requests) == 9
    and all(item.get("inputKind") == "xshm_input" for item in input_requests)
    and all(item.get("inputSettings") == {"screen": 0, "show_cursor": False} for item in input_requests),
    {"count": len(input_requests), "kinds": sorted({item.get("inputKind") for item in input_requests})},
)
by_scene: dict[str, list[dict]] = {}
for item in input_requests:
    by_scene.setdefault(item.get("sceneName", ""), []).append(item)
focus_scenes = ["KMP/TerminalFocus", "KMP/ChronoFocus", "KMP/ProofFocus", "KMP/CTAFocus"]
check(
    "obs_focus_primary_plus_live_inset",
    all(
        len(by_scene.get(scene, [])) == 2
        and all(item.get("inputKind") == "xshm_input" for item in by_scene[scene])
        for scene in focus_scenes
    ),
    {scene: [item.get("inputName") for item in by_scene.get(scene, [])] for scene in focus_scenes},
)
check("obs_start_stop_verified", any("StartRecord" in json.dumps(row) for row in obs_wire) and any("StopRecord" in json.dumps(row) for row in obs_wire), "obs-websocket.jsonl")
obs_stop = json.loads((run / "obs-stop.json").read_text())
expected_advance_ns = round(1_000_000_000 / picture_fps)
record_start_ns = int(obs_stop.get("record_start_monotonic_ns", "0"))
nominal_target_ns = int(obs_stop.get("nominal_target_monotonic_ns", "0"))
stop_target_ns = int(obs_stop.get("target_monotonic_ns", "0"))
check(
    "obs_stop_one_frame_advance",
    obs_stop.get("scheduled_stop_advance_frames") == 1
    and float(obs_stop.get("picture_contract_fps", 0)) == picture_fps
    and int(obs_stop.get("scheduled_stop_advance_ns", "0")) == expected_advance_ns
    and abs(float(obs_stop.get("scheduled_stop_advance_ms", 0)) - expected_advance_ns / 1_000_000) <= 0.000001
    and nominal_target_ns == record_start_ns + int(scenario["duration_ms"]) * 1_000_000
    and stop_target_ns == nominal_target_ns - expected_advance_ns,
    {
        "picture_contract_fps": obs_stop.get("picture_contract_fps"),
        "advance_frames": obs_stop.get("scheduled_stop_advance_frames"),
        "advance_ms": obs_stop.get("scheduled_stop_advance_ms"),
        "advance_ns": obs_stop.get("scheduled_stop_advance_ns"),
        "target_monotonic_ns": obs_stop.get("target_monotonic_ns"),
        "nominal_target_monotonic_ns": obs_stop.get("nominal_target_monotonic_ns"),
    },
)

master = next((item for item in edl.get("masters", []) if item.get("id") == scenario["id"]), None)
expected_schedule = (master or {}).get("obs_schedule") or [{"at_ms": 0, "scene": "KMP/Wide"}]
actual_schedule = json_lines(run / "obs-scene-schedule.jsonl")
actual_projection = [
    {"at_ms": item.get("requested_at_ms"), "scene": item.get("scene")}
    for item in actual_schedule
]
check("obs_schedule_matches_edl", actual_projection == expected_schedule, {"expected": expected_schedule, "actual": actual_projection})
check(
    "obs_schedule_timing",
    bool(actual_schedule) and all(0 <= float(item.get("lateness_ms", -1)) <= 250 for item in actual_schedule),
    [item.get("lateness_ms") for item in actual_schedule],
)

anchors = json_lines(run / "anchors.jsonl")
anchor_names = [item.get("anchor") for item in anchors]
required_anchors = {
    "fresh-process-same-why": {
        "hook_visible", "memory_write_committed", "viewer_decision_visible", "viewer_evidence_visible",
        "process_1_exit", "process_2_spawn", "question_2_visible",
        "recovered_decision_visible", "recovered_evidence_visible", "end_card_visible",
    },
    "two-processes-one-memory": {
        "hook_visible", "process_a_write_committed", "process_b_spawn", "shared_store_recall_visible",
        "process_a_decision_visible", "process_b_question_visible",
        "process_b_recovered_view_visible", "viewer_decision_visible", "viewer_evidence_visible",
        "recovered_decision_visible", "recovered_evidence_visible", "end_card_visible",
    },
    "keep-the-wrong-turn": {
        "hook_visible", "selection_question_visible", "selection_applied_revision", "clock_prism_visible",
        "trace_question_visible", "trace_applied_revision", "hop_1_focus", "hop_2_focus",
        "hop_3_focus", "hop_4_focus", "full_path_visible", "nice_visible", "signature_visible", "end_card_visible",
    },
}.get(scenario["id"], {"recording_clock_locked"})
check("audio_edit_anchors_complete", required_anchors <= set(anchor_names), {"required": sorted(required_anchors), "actual": anchor_names})
anchor_times = [int(item["monotonic_ns"]) for item in anchors]
check(
    "audio_edit_anchors_ordered_and_hashed",
    anchor_times == sorted(anchor_times)
    and all(item.get("evidence") and item.get("source") for item in anchors),
    {"count": len(anchors), "ordered": anchor_times == sorted(anchor_times)},
)
clock_map = json.loads((run / "clock-map.json").read_text())
clock_points = clock_map.get("points", [])
check(
    "monotonic_frame_map",
    clock_map.get("monotonic") is True
    and len(clock_points) == 2
    and int(clock_points[0]["monotonic_ns"]) < int(clock_points[1]["monotonic_ns"])
    and int(clock_points[0]["video_pts_ns"]) < int(clock_points[1]["video_pts_ns"]),
    clock_points,
)

audio_contract = json.loads((run / "audio-contract.json").read_text())
audio_master = audio_contract.get("masters", {}).get(scenario["id"])
if audio_master:
    anchor_pts = {item["anchor"]: int(item["video_pts_ns"]) / 1_000_000_000 for item in anchors}
    cue_resolution = json.loads((run / "audio-cues.json").read_text())
    cue_results = []
    for cue in cue_resolution.get("cues", []):
        visible = cue["visible_anchor"]
        cue_at = float(cue["resolved_at"])
        visible_at = anchor_pts.get(visible)
        lag_frames = None if visible_at is None else (cue_at - visible_at) * 30
        lag_limit = cue.get("max_lag_frames")
        cue_results.append({
            "cue": cue["cue"],
            "planned_at": cue["planned_at"],
            "resolved_at": cue_at,
            "visible_anchor": visible,
            "visible_at": visible_at,
            "lag_frames": lag_frames,
            "passed": visible_at is not None
            and cue_at >= visible_at
            and (lag_limit is None or lag_frames <= float(lag_limit) + 1e-6),
        })
    check(
        "audio_cues_do_not_precede_visible_anchors",
        cue_resolution.get("audio_contract_sha256") == sha256(run / "audio-contract.json")
        and cue_resolution.get("anchors_sha256") == sha256(run / "anchors.jsonl")
        and bool(cue_results)
        and all(item["passed"] for item in cue_results),
        cue_results,
    )

preflight = json.loads((run / "readability-preflight.json").read_text())
preflight_frames = preflight.get("frames", [])
check(
    "mobile_390_review_frames",
    len(preflight_frames) == len(expected_schedule)
    and all(item.get("mobile", {}).get("width") == 390 for item in preflight_frames)
    and [item.get("scene") for item in preflight_frames] == [item["scene"] for item in expected_schedule],
    {"frame_count": len(preflight_frames), "scenes": [item.get("scene") for item in preflight_frames]},
)
check(
    "human_readability_gate_disclosed",
    preflight.get("measurement_status") == "preparatory_only_not_cap_height_acceptance"
    and preflight.get("muted_panel_status") == "pending_5_of_5",
    {"measurement_status": preflight.get("measurement_status"), "muted_panel_status": preflight.get("muted_panel_status")},
)

windows = (run / "window-tree.txt").read_text(errors="replace")
check("real_terminal_window", "KMP_CAPTURE_TERMINAL" in windows, "window-tree.txt")
check("real_chromium_window", "ChronoLoom" in windows or "Google Chrome" in windows or "Chromium" in windows, "window-tree.txt")
check("pty_transcript", (run / "pty.typescript").stat().st_size > 0 and (run / "pty.timing").stat().st_size > 0, [(run / "pty.typescript").stat().st_size, (run / "pty.timing").stat().st_size])

obs_logs = "\n".join(path.read_text(errors="replace") for path in (run / "obs-config" / "obs-studio" / "logs").glob("*.txt"))
check("obs_30_and_websocket", "OBS 30.0.2" in obs_logs and "obs-websocket" in obs_logs, "obs-studio/logs")
check(
    "obs_x264_zerolatency_profile",
    "tune: zerolatency" in obs_logs
    and "bframes = 0" in obs_logs
    and "rc-lookahead = 0" in obs_logs
    and "sync-lookahead = 0" in obs_logs,
    "obs-studio/logs",
)
check("no_desktop_audio_source", "source: 'Desktop Audio'" not in obs_logs, "OBS scene has no desktop audio source")

result = {
    "scenario_id": scenario["id"],
    "passed": all(item["passed"] for item in checks),
    "checks": checks,
}
(run / "verification.json").write_text(json.dumps(result, indent=2) + "\n")

excluded = {"evidence-manifest.json"}
files = []
for file in sorted(run.rglob("*")):
    if not file.is_file() or file.name in excluded:
        continue
    relative = file.relative_to(run).as_posix()
    files.append({"path": relative, "bytes": file.stat().st_size, "sha256": sha256(file)})
manifest = {
    "contract": "kmp.obs-evidence-pack.v1",
    "scenario_id": scenario["id"],
    "recording": {"path": "obs-recording.mkv", "sha256": sha256(recording)},
    "files": files,
}
(run / "evidence-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

for item in checks:
    print(f"{'ok' if item['passed'] else 'FAIL':4} {item['name']}")
if not result["passed"]:
    raise SystemExit("OBS capture verification failed")
print(f"evidence pack verified: {run}")
