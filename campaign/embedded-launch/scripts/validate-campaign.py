#!/usr/bin/env python3
"""Validate KMP Embedded campaign claims, timing and visual governance."""

from __future__ import annotations

import hashlib
import json
import pathlib
import sys

import jsonschema


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
SCHEMA = CAMPAIGN / "schema" / "campaign-brief.schema.json"
SHARED_SCHEMA = pathlib.Path(
    "/home/gx10a/Documents/ai/kmp-campaign-agents/shared/campaign-brief.schema.json"
)


def fail(message: str) -> None:
    raise SystemExit(f"campaign validation failed: {message}")


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    jsonschema.Draft202012Validator(schema).validate(brief)
    if SHARED_SCHEMA.is_file() and digest(SCHEMA) != digest(SHARED_SCHEMA):
        fail("repository schema snapshot differs from the shared campaign schema")

    edl = json.loads((CAMPAIGN / "edl.json").read_text(encoding="utf-8"))
    audio = json.loads((CAMPAIGN / "audio" / "contract.json").read_text(encoding="utf-8"))
    claims = json.loads((CAMPAIGN / "claims.json").read_text(encoding="utf-8"))
    scenarios = json.loads((CAMPAIGN / "scenario-contracts.json").read_text(encoding="utf-8"))
    masters = {item["id"]: item for item in brief["masters"]}
    edit_masters = {item["id"]: item for item in edl["masters"]}
    scenario_masters = {item["id"]: item for item in scenarios["masters"]}
    audio_masters = audio["masters"]
    if set(masters) != set(edit_masters):
        fail("campaign.json and edl.json schedule different masters")
    if set(masters) != set(scenario_masters):
        fail("scenario-contracts.json does not specify every scheduled master")
    if set(masters) != set(audio_masters):
        fail("audio contract does not specify every scheduled master")

    cue_durations: dict[str, float] = {}
    cue_lines = (CAMPAIGN / "audio" / "cues.tsv").read_text(encoding="utf-8").splitlines()
    if not cue_lines or cue_lines[0] != "cue\tstart\tend":
        fail("audio cue palette has a non-canonical header")
    for line in cue_lines[1:]:
        name, start, end = line.split("\t")
        duration = float(end) - float(start)
        if duration <= 0 or name in cue_durations:
            fail(f"audio cue {name} has an invalid palette range")
        cue_durations[name] = duration

    forbidden_host_copy = ("codex", "claude", "agent a", "agent b", "two agents")
    for master_id, master in masters.items():
        beats = master["beats"]
        if not 5 <= len(beats) <= 8:
            fail(f"{master_id} has {len(beats)} narrative beats; expected 5..8")
        cursor = 0.0
        for beat in beats:
            if abs(float(beat["start"]) - cursor) > 0.0001:
                fail(f"{master_id} has a timing gap or overlap at {cursor}")
            if float(beat["end"]) <= float(beat["start"]):
                fail(f"{master_id} has a non-positive beat")
            cursor = float(beat["end"])
            scheduled = f"{beat['copy']} {beat['visual_action']}".lower()
            if any(term in scheduled for term in forbidden_host_copy):
                fail(f"{master_id} contains unsupported host/agent branding")
        if abs(cursor - float(master["duration_seconds"])) > 0.0001:
            fail(f"{master_id} ends at {cursor}, not {master['duration_seconds']}")
        captions = CAMPAIGN / str(master["captions"])
        if not captions.is_file():
            fail(f"missing captions for {master_id}: {captions}")
        if "github.com/underpass-ai/kmp" not in captions.read_text(encoding="utf-8"):
            fail(f"{master_id} captions omit the final CTA")

        edit = edit_masters[master_id]
        if float(edit["duration_seconds"]) != float(master["duration_seconds"]):
            fail(f"{master_id} duration differs between brief and EDL")
        if pathlib.Path(edit["raw_picture"]).name != pathlib.Path(master["picture_source"]).name:
            fail(f"{master_id} picture source differs between brief and EDL")
        timeline = edit["timeline"]
        if timeline[0]["start"] != 0.0 or timeline[-1]["end"] != master["duration_seconds"]:
            fail(f"{master_id} EDL does not cover the full duration")
        for before, after in zip(timeline, timeline[1:]):
            if abs(float(before["end"]) - float(after["start"])) > 0.0001:
                fail(f"{master_id} EDL has a timing gap or overlap")
        if timeline[-1]["scene"] != "KMP/CTAFocus":
            fail(f"{master_id} does not end in the mobile-safe CTA scene")
        schedule = edit.get("obs_schedule")
        if not schedule or schedule[0] != {"at_ms": 0, "scene": timeline[0]["scene"]}:
            fail(f"{master_id} OBS schedule does not start with the first EDL scene")
        expected_schedule = []
        previous_scene = None
        for item in timeline:
            if item["scene"] != previous_scene:
                expected_schedule.append({
                    "at_ms": round(float(item["start"]) * 1000),
                    "scene": item["scene"],
                })
                previous_scene = item["scene"]
        if schedule != expected_schedule:
            fail(f"{master_id} OBS schedule does not match EDL scene transitions")
        if any(item["scene"] not in edl["picture_contract"]["mobile_safe_scenes"] for item in schedule):
            fail(f"{master_id} OBS schedule uses a scene outside the mobile-safe contract")

        audio_master = audio_masters[master_id]
        scheduled_cues = [(str(name), float(at)) for name, at in edit["audio_cues"]]
        contracted_cues = [
            (str(item["cue"]), float(item["at"])) for item in audio_master["cue_anchors"]
        ]
        if scheduled_cues != contracted_cues:
            fail(f"{master_id} EDL cues differ from the audio anchor contract")
        silence = [(float(start), float(end)) for start, end in audio_master["digital_silence"]]
        if not silence or silence != sorted(silence):
            fail(f"{master_id} audio silence intervals are missing or unordered")
        for start, end in silence:
            if not 0 <= start < end <= float(master["duration_seconds"]):
                fail(f"{master_id} has a silence interval outside its duration")
        final_start, final_end = silence[-1]
        required_final = float(edl["picture_contract"]["readability_acceptance"]["required_final_silence_seconds"])
        if (
            final_end != float(master["duration_seconds"])
            or final_end - final_start < required_final - 0.0001
        ):
            fail(f"{master_id} does not preserve the required final audio silence")
        for item in audio_master["cue_anchors"]:
            cue = str(item["cue"])
            if cue not in cue_durations:
                fail(f"{master_id} schedules unknown audio cue {cue}")
            cue_start = float(item["at"])
            cue_end = cue_start + cue_durations[cue]
            if cue_start < float(item["not_before"]):
                fail(f"{master_id} schedules {cue} before its visible event")
            if not str(item.get("visible_anchor", "")):
                fail(f"{master_id} cue {cue} has no visible-event anchor")
            for silent_start, silent_end in silence:
                if cue_start < silent_end - 0.0001 and cue_end > silent_start + 0.0001:
                    fail(f"{master_id} cue {cue} overlaps digital silence")

        scenario = scenario_masters[master_id]
        if scenario["duration_ms"] != round(float(master["duration_seconds"]) * 1000):
            fail(f"{master_id} scenario duration differs from campaign.json")
        if pathlib.Path(scenario["scenario_path"]).name != f"{master_id}.json":
            fail(f"{master_id} scenario path is not canonical")
        for event in scenario["required_events"]:
            points = event.get("window_ms", [event.get("at_ms"), event.get("at_ms")])
            if points[0] is None or not 0 <= points[0] <= points[1] <= scenario["duration_ms"]:
                fail(f"{master_id} scenario has an event outside its duration")

    fresh = scenario_masters["fresh-process-same-why"]
    if len(fresh["processes"]) != 2 or fresh["processes"][0]["store_id"] != fresh["processes"][1]["store_id"]:
        fail("fresh-process scenario does not specify two processes on one store")
    if fresh["processes"][1]["autostart"] is not False:
        fail("fresh-process session-02 starts before session-01 exits")
    shared = scenario_masters["two-processes-one-memory"]
    if len(shared["processes"]) != 2 or not all(item["autostart"] for item in shared["processes"]):
        fail("two-process scenario does not start two live processes")
    if shared["processes"][0]["store_id"] != shared["processes"][1]["store_id"]:
        fail("two-process scenario does not share one store")
    wrong = scenario_masters["keep-the-wrong-turn"]
    if wrong.get("fixture_label") != "DETERMINISTIC PRODUCT FIXTURE":
        fail("wrong-turn scenario omits its visible fixture label")
    if audio["source_policy"].get("captured_obs_audio_allowed") is not False:
        fail("audio contract permits captured OBS audio")
    if audio["pcm"] != {
        "sample_rate_hz": 48000,
        "channels": 2,
        "bits_per_sample": 24,
        "codec": "pcm_s24le",
        "canonical_hash_format": "s24le-48000hz-stereo-interleaved",
    }:
        fail("audio PCM contract drifted from canonical 48 kHz/24-bit stereo")

    allowed_master_ids = set(masters) | {"all"}
    for claim in claims["claims"]:
        if claim["master_id"] not in allowed_master_ids:
            fail(f"claim {claim['id']} names an unscheduled master")
    if "deterministic product fixture" not in claims["fixture_disclosure"].lower():
        fail("claim ledger does not disclose the deterministic fixture")

    derivative = edl["readme_derivative"]
    if derivative["source_master_id"] != "fresh-process-same-why":
        fail("README GIF is not sourced from campaign master 1")
    if derivative["other_gif_derivatives_allowed"] is not False:
        fail("more than one GIF derivative is permitted")
    derive = (CAMPAIGN / "scripts" / "derive-readme-gif.sh").read_text(encoding="utf-8")
    if "fresh-process-same-why.mp4" not in derive or "kmp-agent-loom.gif" not in derive:
        fail("GIF derivation script disagrees with the EDL")
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    if readme.count("docs/assets/kmp-agent-loom.gif") != 1:
        fail("root README must embed exactly one campaign GIF")

    browser_probe = (ROOT / "scripts/demo/record-chronoloom-gifs.js").read_text(
        encoding="utf-8"
    )
    for forbidden in (
        "agentComposite", "capture-terminal", "CODEX × KMP", "CLAUDE × KMP",
        "Two agents.\\nOne SQLite WAL store.",
    ):
        if forbidden in browser_probe:
            fail(f"browser probe still contains forbidden terminal/host source: {forbidden}")

    social = (CAMPAIGN / "social.md").read_text(encoding="utf-8")
    for master_id in masters:
        attachment = f"docs/assets/campaign/kmp-embedded/{master_id}.mp4"
        if social.count(attachment) != 1:
            fail(f"social copy does not name exactly one {master_id} attachment")
    mode = None
    post_lines: list[str] = []
    social_posts: list[str] = []
    for line in social.splitlines():
        if line in {"Post 1:", "Reply:"}:
            if post_lines:
                social_posts.append("\n".join(post_lines))
            post_lines = []
            mode = line
        elif line.startswith("Alt text:"):
            if post_lines:
                social_posts.append("\n".join(post_lines))
            post_lines = []
            mode = None
        elif mode and line.startswith(">"):
            post_lines.append(line[1:].strip())
    if post_lines:
        social_posts.append("\n".join(post_lines))
    if len(social_posts) != len(masters) * 2:
        fail("social.md must contain one exact post and one exact reply per master")
    if any(len(post) > 280 for post in social_posts):
        fail("a launch post exceeds 280 characters")

    print("campaign validation: passed")


if __name__ == "__main__":
    try:
        main()
    except jsonschema.ValidationError as error:
        print(f"campaign schema validation failed: {error.message}", file=sys.stderr)
        raise SystemExit(1) from error
