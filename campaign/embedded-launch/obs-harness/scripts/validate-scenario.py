#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import re
import sys

import jsonschema


def fail(message: str) -> None:
    raise SystemExit(f"scenario validation failed: {message}")


if len(sys.argv) != 2:
    fail("usage: validate-scenario.py SCENARIO")

path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
schema_path = pathlib.Path(__file__).resolve().parents[1] / "scenario.schema.json"
jsonschema.Draft202012Validator(json.loads(schema_path.read_text())).validate(data)
required = {"id", "title", "about", "duration_ms", "bootstrap", "steps"}
missing = sorted(required - data.keys())
if missing:
    fail(f"missing fields: {', '.join(missing)}")
optional = {"fixture_label", "stores", "processes"}
if set(data) - (required | optional):
    fail(f"unknown top-level fields: {', '.join(sorted(set(data) - required - optional))}")
if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", data["id"]):
    fail("id is not kebab-case")
if not isinstance(data["duration_ms"], int) or not 3000 <= data["duration_ms"] <= 180000:
    fail("duration_ms must be 3000..180000")
stores = data.get("stores", [{"id": "default"}])
processes = data.get("processes", [{"id": "process", "store_id": stores[0]["id"], "autostart": True, "browser": True}])
if not isinstance(stores, list) or not stores:
    fail("stores must be a non-empty list")
if not isinstance(processes, list) or not 1 <= len(processes) <= 8:
    fail("processes must contain 1..8 entries")
store_ids = [item.get("id") for item in stores if isinstance(item, dict)]
process_ids = [item.get("id") for item in processes if isinstance(item, dict)]
if len(store_ids) != len(stores) or len(set(store_ids)) != len(store_ids) or not all(re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", item or "") for item in store_ids):
    fail("store ids must be unique kebab-case strings")
if len(process_ids) != len(processes) or len(set(process_ids)) != len(process_ids) or not all(re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", item or "") for item in process_ids):
    fail("process ids must be unique kebab-case strings")
for index, item in enumerate(processes):
    if item.get("store_id") not in store_ids:
        fail(f"processes[{index}] references an unknown store")
if sum(bool(item.get("browser")) for item in processes) > 1:
    fail("at most one process may be the initial browser target")

default_process_id = process_ids[0]
last = -1
for lane in ("bootstrap", "steps"):
    if not isinstance(data[lane], list):
        fail(f"{lane} must be a list")
    for index, step in enumerate(data[lane]):
        if not isinstance(step, dict) or "type" not in step or "at_ms" not in step:
            fail(f"{lane}[{index}] needs type and at_ms")
        if lane == "bootstrap" and step["type"] != "tool":
            fail(f"{lane}[{index}] must be a tool")
        if step["type"] not in {"say", "tool", "process", "hold"}:
            fail(f"{lane}[{index}] has unknown type")
        if lane == "steps":
            if step["at_ms"] < last:
                fail("steps must be ordered by at_ms")
            if step["at_ms"] > data["duration_ms"]:
                fail("step occurs after duration_ms")
            last = step["at_ms"]
        if step["type"] == "tool":
            if not re.fullmatch(r"kmp_[a-z_]+", step.get("name", "")):
                fail(f"{lane}[{index}] has an invalid tool name")
            if not isinstance(step.get("arguments"), dict):
                fail(f"{lane}[{index}] tool arguments must be an object")
            if step.get("process_id", default_process_id) not in process_ids:
                fail(f"{lane}[{index}] references an unknown process")
        if step["type"] == "process":
            if lane == "bootstrap":
                fail(f"{lane}[{index}] cannot manage processes")
            if step.get("action") not in {"start", "stop", "switch_browser"}:
                fail(f"{lane}[{index}] has an invalid process action")
            if step.get("process_id") not in process_ids:
                fail(f"{lane}[{index}] references an unknown process")
        if step["type"] == "say" and step.get("speaker") not in {"user", "process", "kmp"}:
            fail(f"{lane}[{index}] has an invalid speaker")

campaign_root = pathlib.Path(__file__).resolve().parents[2]
contracts_path = campaign_root / "scenario-contracts.json"
edl_path = campaign_root / "edl.json"
if contracts_path.is_file() and edl_path.is_file():
    contracts = json.loads(contracts_path.read_text())
    contract = next((item for item in contracts.get("masters", []) if item["id"] == data["id"]), None)
    if contract:
        for field in ("about", "duration_ms", "stores", "processes"):
            if data.get(field) != contract.get(field):
                fail(f"{field} differs from scenario-contracts.json")
        if data.get("fixture_label") != contract.get("fixture_label"):
            fail("fixture_label differs from scenario-contracts.json")
        for required_event in contract["required_events"]:
            low, high = required_event.get("window_ms", [required_event.get("at_ms"), required_event.get("at_ms")])
            candidates = [step for step in data["steps"] if low <= step["at_ms"] <= high]
            event_type = required_event["type"]
            if event_type == "say":
                candidates = [
                    step for step in candidates
                    if step["type"] == "say"
                    and step.get("speaker") == required_event["speaker"]
                    and step.get("text") == required_event["text"]
                ]
            elif event_type == "tool":
                candidates = [
                    step for step in candidates
                    if step["type"] == "tool"
                    and step.get("process_id", default_process_id) == required_event["process_id"]
                    and step.get("name") == required_event["name"]
                ]
            elif event_type == "process":
                candidates = [
                    step for step in candidates
                    if step["type"] == "process"
                    and step.get("process_id") == required_event["process_id"]
                    and step.get("action") == required_event["action"]
                ]
            elif event_type == "hold":
                candidates = [step for step in candidates if step["type"] == "hold"]
            if not candidates:
                fail(f"required event missing: {required_event}")
        called = [step["name"] for step in data["bootstrap"] + data["steps"] if step["type"] == "tool"]
        if any(name in {"kmp_export", "kmp_import", "export", "import"} for name in called):
            fail("campaign scenarios may not export or import")
        if data["id"] == "keep-the-wrong-turn":
            ingest = next((step for step in data["bootstrap"] if step.get("name") == "kmp_ingest"), None)
            memory = (ingest or {}).get("arguments", {}).get("memory", {})
            if [len(memory.get(key, [])) for key in ("entries", "relations", "evidence")] != [7, 7, 3]:
                fail("wrong-turn fixture must contain exactly 7 entries, 7 relations and 3 evidence records")
        edl = json.loads(edl_path.read_text())
        master = next(item for item in edl["masters"] if item["id"] == data["id"])
        if round(master["duration_seconds"] * 1000) != data["duration_ms"]:
            fail("scenario duration differs from EDL")
        if any(item["at_ms"] >= data["duration_ms"] for item in master["obs_schedule"]):
            fail("EDL scene change falls outside scenario duration")
        for event in master["obs_schedule"]:
            if event["scene"] not in {"KMP/TerminalFocus", "KMP/CTAFocus"}:
                continue
            if not any(
                step["at_ms"] == event["at_ms"] and step["type"] in {"say", "process"}
                for step in data["steps"]
            ):
                fail(
                    f"{event['scene']} at {event['at_ms']}ms needs an exact say/process "
                    "beat to reset the semantic terminal viewport"
                )
        print(f"campaign contract valid: {data['id']}")

print(f"scenario valid: {data['id']}")
