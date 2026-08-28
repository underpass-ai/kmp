#!/usr/bin/env python3
"""Build, verify, or apply KMP's two versioned guide memories."""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile


PLUGIN_ROOT = pathlib.Path(__file__).resolve().parents[1]
ROOT = PLUGIN_ROOT.parents[1]
EDITORIAL = pathlib.Path(__file__).with_name("editorial.json")
REQUESTS = pathlib.Path(__file__).with_name("guide.requests.json")
BUNDLE = pathlib.Path(__file__).with_name("memory.jsonl")
CAPABILITIES = PLUGIN_ROOT / "capabilities.json"
MAX_BUNDLE_BYTES = 128 * 1024
BASE_TIME = dt.datetime(2026, 8, 28, tzinfo=dt.timezone.utc)

TOOL_VERB = {
    "kmp_ingest": "verb:write",
    "kmp_write_memory": "verb:write",
    "kmp_wake": "verb:wake",
    "kmp_ask": "verb:ask",
    "kmp_goto": "verb:time",
    "kmp_near": "verb:time",
    "kmp_rewind": "verb:time",
    "kmp_forward": "verb:time",
    "kmp_trace": "verb:audit",
    "kmp_inspect": "verb:audit",
    "kmp_view_open": "verb:view",
    "kmp_view_apply_intent": "verb:view",
    "kmp_view_get_state": "verb:view",
}


def fail(message: str) -> None:
    raise SystemExit(f"KMP guide: {message}")


def compact(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def binary_env(data_dir: pathlib.Path | None = None) -> dict[str, str]:
    env = os.environ.copy()
    env["KMP_VIEWER_ADDR"] = "off"
    if data_dir is not None:
        env["KMP_MCP_DATA_DIR"] = str(data_dir)
        env["XDG_CONFIG_HOME"] = str(data_dir / "config")
    return env


def exchange(
    binary: pathlib.Path,
    messages: list[dict[str, object]],
    *,
    data_dir: pathlib.Path | None = None,
) -> dict[int, dict[str, object]]:
    payload = "".join(compact(message) + "\n" for message in messages)
    result = subprocess.run(
        [str(binary)],
        input=payload,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=binary_env(data_dir),
        check=False,
    )
    if result.returncode != 0:
        fail(f"{binary} exited {result.returncode}: {result.stderr.strip()}")
    responses: dict[int, dict[str, object]] = {}
    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            fail(f"the MCP server emitted invalid JSON: {error}: {line!r}")
        identifier = message.get("id")
        if isinstance(identifier, int):
            responses[identifier] = message
    return responses


def initialize(identifier: int = 1) -> dict[str, object]:
    return {
        "jsonrpc": "2.0",
        "id": identifier,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "kmp-guide", "version": "1"},
        },
    }


def live_tools(binary: pathlib.Path) -> list[dict[str, object]]:
    with tempfile.TemporaryDirectory(dir=ROOT / "tmp") as raw_data_dir:
        responses = exchange(
            binary,
            [
                initialize(),
                {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
            ],
            data_dir=pathlib.Path(raw_data_dir),
        )
    listing = responses.get(2, {}).get("result", {})
    tools = listing.get("tools") if isinstance(listing, dict) else None
    if not isinstance(tools, list):
        fail("tools/list did not return a tool inventory")
    if any(not isinstance(tool, dict) for tool in tools):
        fail("tools/list returned a malformed tool definition")
    return sorted(tools, key=lambda tool: str(tool.get("name", "")))


def iso_time(sequence: int) -> str:
    value = BASE_TIME + dt.timedelta(minutes=sequence - 1)
    return value.isoformat().replace("+00:00", "Z")


def full_ref(about: str, suffix: str) -> str:
    return f"{about}:{suffix}"


def build_about(
    source: dict[str, object],
    guide_version: str,
    observed_at: str,
    tools: list[dict[str, object]],
) -> dict[str, object]:
    about = str(source["about"])
    audience = str(source["audience"])
    raw_entries = list(source["entries"])
    if audience == "agent":
        for tool in tools:
            name = str(tool.get("name", ""))
            description = str(tool.get("description", "")).strip()
            if name not in TOOL_VERB or not description:
                fail(f"cannot place live tool {name!r} in the agent guide")
            raw_entries.append(
                {
                    "id": f"tool:{name}",
                    "kind": "reference",
                    "depth": "advanced",
                    "text": f"LIVE TOOL {name}. {description}",
                    "evidence": description,
                    "generated_tool": name,
                }
            )

    dimensions = [
        {"id": "timeline", "kind": "timeline", "title": "Guide order"},
        {
            "id": f"audience-{audience}",
            "kind": "audience",
            "title": audience,
        },
        {"id": "depth-basic", "kind": "depth", "title": "Basic"},
        {"id": "depth-advanced", "kind": "depth", "title": "Advanced"},
    ]
    entries: list[dict[str, object]] = []
    evidence: list[dict[str, object]] = []
    for sequence, raw in enumerate(raw_entries, 1):
        suffix = str(raw["id"])
        depth = str(raw["depth"])
        entry_ref = full_ref(about, suffix)
        source_kind = "tools/list" if "generated_tool" in raw else "editorial"
        entry = {
            "id": entry_ref,
            "kind": str(raw["kind"]),
            "text": str(raw["text"]),
            "coordinates": [
                {
                    "dimension": "timeline",
                    "scope_id": "timeline",
                    "sequence": sequence,
                    "occurred_at": iso_time(sequence),
                    "observed_at": iso_time(sequence),
                },
                {
                    "dimension": "audience",
                    "scope_id": f"audience-{audience}",
                    "sequence": sequence,
                },
                {
                    "dimension": "depth",
                    "scope_id": f"depth-{depth}",
                    "sequence": sequence,
                },
            ],
            "metadata": {
                "audience": audience,
                "depth": depth,
                "guide_version": guide_version,
                "source": source_kind,
            },
        }
        if "generated_tool" in raw:
            entry["metadata"]["tool_name"] = str(raw["generated_tool"])
        entries.append(entry)
        evidence.append(
            {
                "id": f"evidence:{about}:{suffix}",
                "supports": [entry_ref],
                "text": str(raw["evidence"]),
                "source": f"KMP {source_kind} guide source v{guide_version}",
                "time": observed_at,
                "metadata": {"guide_version": guide_version, "audience": audience},
            }
        )

    relations: list[dict[str, object]] = []
    for sequence, raw in enumerate(source["relations"], 1):
        relation = {
            "from": full_ref(about, str(raw["from"])),
            "to": full_ref(about, str(raw["to"])),
            "rel": str(raw["rel"]),
            "class": str(raw["class"]),
            "why": str(raw["why"]),
            "evidence": str(raw["evidence"]),
            "confidence": "high",
            "sequence": sequence,
        }
        relations.append(relation)

    if audience == "agent":
        relation_sequence = len(relations)
        for tool in tools:
            relation_sequence += 1
            name = str(tool["name"])
            description = str(tool["description"]).strip()
            relations.append(
                {
                    "from": full_ref(about, f"tool:{name}"),
                    "to": full_ref(about, TOOL_VERB[name]),
                    "rel": "uses_background",
                    "class": "evidential",
                    "why": "The human-readable verb rule is grounded in this exact live MCP contract.",
                    "evidence": description,
                    "confidence": "high",
                    "sequence": relation_sequence,
                }
            )

    memory = {
        "dimensions": dimensions,
        "entries": entries,
        "relations": relations,
        "evidence": evidence,
    }
    provenance = {
        "source_kind": "derived",
        "source_agent": "kmp-guide-builder",
        "observed_at": observed_at,
        "correlation_id": f"guide:kmp:v{guide_version}",
        "causation_id": "release:guide-sync",
    }
    logical = {"about": about, "memory": memory, "provenance": provenance}
    digest = hashlib.sha256(compact(logical).encode("utf-8")).hexdigest()[:20]
    return {
        **logical,
        "idempotency_key": f"ingest:guide-sync:{guide_version}:{audience}:{digest}",
    }


def build_requests(binary: pathlib.Path) -> list[dict[str, object]]:
    editorial = json.loads(EDITORIAL.read_text(encoding="utf-8"))
    if editorial.get("schema_version") != 1:
        fail("editorial.json has an unsupported schema_version")
    guide_version = str(editorial["guide_version"])
    observed_at = str(editorial["observed_at"])
    tools = live_tools(binary)
    names = [str(tool.get("name", "")) for tool in tools]
    capabilities = json.loads(CAPABILITIES.read_text(encoding="utf-8"))
    expected = sorted(capabilities["mcp_tools"])
    if names != expected:
        fail(f"live tools differ from capabilities.json: live={names}, expected={expected}")
    abouts = editorial.get("abouts")
    if not isinstance(abouts, list) or [about.get("audience") for about in abouts] != [
        "agent",
        "person",
    ]:
        fail("editorial.json must declare the agent guide before the person guide")
    requests = [
        build_about(about, guide_version, observed_at, tools) for about in abouts
    ]
    relation_count = sum(len(request["memory"]["relations"]) for request in requests)
    rich_count = sum(
        1
        for request in requests
        for relation in request["memory"]["relations"]
        if relation.get("why") and relation.get("evidence")
    )
    if relation_count != rich_count:
        fail("every guide relation must carry both why and evidence")
    return requests


def requests_text(requests: list[dict[str, object]]) -> str:
    return json.dumps(requests, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def call_requests(
    binary: pathlib.Path,
    requests: list[dict[str, object]],
    *,
    data_dir: pathlib.Path | None = None,
) -> None:
    messages = [initialize()]
    for identifier, arguments in enumerate(requests, 2):
        messages.append(
            {
                "jsonrpc": "2.0",
                "id": identifier,
                "method": "tools/call",
                "params": {"name": "kmp_ingest", "arguments": arguments},
            }
        )
    responses = exchange(binary, messages, data_dir=data_dir)
    for identifier, request in enumerate(requests, 2):
        response = responses.get(identifier)
        result = response.get("result") if isinstance(response, dict) else None
        if not isinstance(result, dict) or result.get("isError") is not False:
            fail(f"guide ingest failed for {request['about']}: {response!r}")


def binary_version(binary: pathlib.Path) -> str:
    result = subprocess.run(
        [str(binary), "--version"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"cannot read {binary} version: {result.stderr.strip()}")
    fields = result.stdout.strip().split()
    if len(fields) < 2 or fields[0] != "kmp-mcp":
        fail(f"unexpected kmp-mcp version output: {result.stdout!r}")
    return fields[1]


def write_assets(binary: pathlib.Path) -> None:
    requests = build_requests(binary)
    REQUESTS.write_text(requests_text(requests), encoding="utf-8")
    with tempfile.TemporaryDirectory(dir=ROOT / "tmp") as raw_data_dir:
        data_dir = pathlib.Path(raw_data_dir)
        call_requests(binary, requests, data_dir=data_dir)
        generated = data_dir / "guide-memory.jsonl"
        result = subprocess.run(
            [
                str(binary),
                "export",
                str(generated),
                "--about",
                "guide:kmp-agent",
                "--about",
                "guide:kmp",
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=binary_env(data_dir),
            check=False,
        )
        if result.returncode != 0:
            fail(f"guide export failed: {result.stderr.strip()}")
        BUNDLE.write_bytes(generated.read_bytes())
    print(f"KMP guide: wrote {REQUESTS.relative_to(ROOT)} and {BUNDLE.relative_to(ROOT)}")


def assert_tool_result(
    responses: dict[int, dict[str, object]], identifier: int, label: str
) -> dict[str, object]:
    response = responses.get(identifier)
    result = response.get("result") if isinstance(response, dict) else None
    if not isinstance(result, dict) or result.get("isError") is not False:
        fail(f"{label} failed after importing the guide: {response!r}")
    structured = result.get("structuredContent")
    if not isinstance(structured, dict):
        fail(f"{label} returned no structuredContent")
    return structured


def check_assets(binary: pathlib.Path) -> None:
    requests = build_requests(binary)
    expected = requests_text(requests)
    if not REQUESTS.is_file() or REQUESTS.read_text(encoding="utf-8") != expected:
        fail("guide.requests.json is stale; run build-guide.py write with the current binary")
    if not BUNDLE.is_file():
        fail("memory.jsonl is missing; run build-guide.py write")
    if BUNDLE.stat().st_size > MAX_BUNDLE_BYTES:
        fail(
            f"memory.jsonl is {BUNDLE.stat().st_size} bytes; budget is {MAX_BUNDLE_BYTES}"
        )
    lines = [line for line in BUNDLE.read_text(encoding="utf-8").splitlines() if line]
    try:
        header = json.loads(lines[0])
    except (IndexError, json.JSONDecodeError) as error:
        fail(f"memory.jsonl has no valid format-2 header: {error}")
    expected_abouts = ["guide:kmp", "guide:kmp-agent"]
    for actual, wanted, label in (
        (header.get("bundle_format"), 2, "bundle format"),
        (header.get("event_count"), 2, "event count"),
        (header.get("abouts"), expected_abouts, "about inventory"),
        (header.get("kernel_version"), binary_version(binary), "kernel version"),
    ):
        if actual != wanted:
            fail(f"memory.jsonl {label} differs: {actual!r} != {wanted!r}")

    with tempfile.TemporaryDirectory(dir=ROOT / "tmp") as raw_data_dir:
        data_dir = pathlib.Path(raw_data_dir)
        imported = subprocess.run(
            [str(binary), "import", str(BUNDLE)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=binary_env(data_dir),
            check=False,
        )
        if imported.returncode != 0:
            fail(f"memory.jsonl does not import: {imported.stderr.strip()}")
        responses = exchange(
            binary,
            [
                initialize(),
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_wake",
                        "arguments": {"about": "guide:kmp-agent"},
                    },
                },
                {
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_ask",
                        "arguments": {
                            "about": "guide:kmp",
                            "question": "What is the difference between occurred and observed time?",
                            "answer_policy": "evidence_or_unknown",
                            "budget": {"detail": "full", "max_bytes": 10000},
                        },
                    },
                },
                {
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_ingest",
                        "arguments": {
                            "about": "project:guide-isolation-probe",
                            "idempotency_key": "ingest:guide-isolation-probe:1",
                            "memory": {
                                "dimensions": [
                                    {"id": "timeline", "kind": "timeline"}
                                ],
                                "entries": [
                                    {
                                        "id": "project:guide-isolation-probe:decision:own",
                                        "kind": "decision",
                                        "text": "This project entry is not guide memory.",
                                        "coordinates": [
                                            {
                                                "dimension": "timeline",
                                                "scope_id": "timeline",
                                                "sequence": 1,
                                            }
                                        ],
                                    }
                                ],
                            },
                        },
                    },
                },
                {
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_wake",
                        "arguments": {"about": "project:guide-isolation-probe"},
                    },
                },
                {
                    "jsonrpc": "2.0",
                    "id": 6,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_ask",
                        "arguments": {
                            "about": "guide:kmp-agent",
                            "question": "When should I use the wake verb?",
                            "answer_policy": "evidence_or_unknown",
                            "budget": {"detail": "full", "max_bytes": 10000},
                        },
                    },
                },
            ],
            data_dir=data_dir,
        )
        agent = assert_tool_result(responses, 2, "agent guide wake")
        clocks = assert_tool_result(responses, 3, "human guide clock Ask")
        assert_tool_result(responses, 4, "project isolation seed")
        isolated = assert_tool_result(responses, 5, "project isolation wake")
        verb = assert_tool_result(responses, 6, "agent guide verb Ask")
        if "guide:kmp-agent" not in compact(agent) or "VERB wake" not in compact(verb):
            fail("agent guide does not expose its wake packet and explicit verb contract")
        clock_packet = compact(clocks).lower()
        if clocks.get("answer") == "UNKNOWN" or not all(
            word in clock_packet for word in ("occurred", "observed")
        ):
            fail("human guide cannot answer the occurred-versus-observed question")
        if "guide:kmp" in compact(isolated):
            fail("the guide leaked into a project-scoped wake")

    # Exercise the lifecycle path separately from the empty-store import. A
    # previous guide version must advance to the shipped version exactly once,
    # and replaying the shipped version must be a true idempotent no-op.
    previous_requests = copy.deepcopy(requests)
    previous_version = "0.0.0-update-fixture"
    previous_time = "2026-08-27T00:00:00Z"
    for request in previous_requests:
        memory = request["memory"]
        entries = memory["entries"]
        audience = entries[0]["metadata"]["audience"]
        request["idempotency_key"] = (
            f"ingest:guide-sync:{previous_version}:{audience}:update-fixture"
        )
        request["provenance"]["observed_at"] = previous_time
        request["provenance"]["correlation_id"] = (
            f"guide:kmp:v{previous_version}"
        )
        for entry in entries:
            entry["metadata"]["guide_version"] = previous_version
            for coordinate in entry["coordinates"]:
                if coordinate["dimension"] == "timeline":
                    coordinate["occurred_at"] = previous_time
                    coordinate["observed_at"] = previous_time
        for item in memory["evidence"]:
            item["source"] = (
                f"KMP {item['metadata']['audience']} guide update fixture"
            )
            item["time"] = previous_time
            item["metadata"]["guide_version"] = previous_version
        if request["about"] == "guide:kmp":
            entries[0]["text"] = "STALE GUIDE UPDATE FIXTURE"

    with tempfile.TemporaryDirectory(dir=ROOT / "tmp") as raw_data_dir:
        data_dir = pathlib.Path(raw_data_dir)
        call_requests(binary, previous_requests, data_dir=data_dir)
        call_requests(binary, requests, data_dir=data_dir)
        call_requests(binary, requests, data_dir=data_dir)
        upgraded = data_dir / "upgraded-guide.jsonl"
        exported = subprocess.run(
            [
                str(binary),
                "export",
                str(upgraded),
                "--about",
                "guide:kmp-agent",
                "--about",
                "guide:kmp",
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=binary_env(data_dir),
            check=False,
        )
        if exported.returncode != 0:
            fail(f"updated guide export failed: {exported.stderr.strip()}")
        with upgraded.open(encoding="utf-8") as handle:
            upgraded_header = json.loads(handle.readline())
        if upgraded_header.get("event_count") != 4:
            fail(
                "guide update/replay produced "
                f"{upgraded_header.get('event_count')} events, expected 4"
            )
        state = exchange(
            binary,
            [
                initialize(),
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "kmp_inspect",
                        "arguments": {
                            "about": "guide:kmp",
                            "ref": "guide:kmp:welcome",
                            "include": {"details": True},
                        },
                    },
                },
            ],
            data_dir=data_dir,
        )
        current_welcome = assert_tool_result(state, 2, "updated human guide")
        rendered_welcome = compact(current_welcome)
        if previous_version in rendered_welcome or "STALE GUIDE" in rendered_welcome:
            fail("guide update left the previous human welcome visible")
        if str(requests[1]["memory"]["entries"][0]["text"]) not in rendered_welcome:
            fail("guide update did not make the shipped human welcome current")
    print(
        "KMP guide: requests match tools/list; bundle imports; agent, human, "
        "isolation and deterministic-update probes pass"
    )


def apply_assets(binary: pathlib.Path) -> None:
    requests = build_requests(binary)
    if not REQUESTS.is_file() or REQUESTS.read_text(encoding="utf-8") != requests_text(requests):
        fail("installed guide requests do not match this engine's live tool surface")
    call_requests(binary, requests)
    print("KMP guide: guide:kmp-agent and guide:kmp converged")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("write", "check", "apply"))
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    binary = args.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        fail(f"binary is not executable: {binary}")
    (ROOT / "tmp").mkdir(exist_ok=True)
    if args.command == "write":
        write_assets(binary)
        check_assets(binary)
    elif args.command == "check":
        check_assets(binary)
    else:
        apply_assets(binary)


if __name__ == "__main__":
    main()
