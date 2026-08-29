#!/usr/bin/env python3
"""Freeze or verify the exact KMP binary and MCP surface used by the campaign."""

from __future__ import annotations

import hashlib
import argparse
import json
import os
import pathlib
import subprocess
import sys
import tempfile


CAMPAIGN = pathlib.Path(__file__).resolve().parents[1]
ROOT = CAMPAIGN.parents[1]
PACK = CAMPAIGN / "evidence-pack"
PRODUCT = PACK / "product"
RELEASE = PACK / "release"
CANDIDATE_CONTRACT = "kmp.release-candidate.v1"


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, **kwargs)


def tools_list(binary: pathlib.Path) -> dict[str, object]:
    (ROOT / "tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="campaign-product-probe.", dir=ROOT / "tmp") as data_dir:
        environment = os.environ.copy()
        environment.update(
            {
                "KMP_MCP_BACKEND": "embedded",
                "KMP_MCP_ENGINE": "sqlite",
                "KMP_MCP_DATA_DIR": data_dir,
                "KMP_VIEWER_ADDR": "off",
                "RUST_LOG": "error",
            }
        )
        request = '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}\n'
        result = run(
            [str(binary)],
            input=request,
            capture_output=True,
            env=environment,
            timeout=30,
        )
    responses = []
    for line in result.stdout.splitlines():
        try:
            responses.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    response = next((item for item in responses if item.get("id") == 1), None)
    if not isinstance(response, dict) or not isinstance(response.get("result"), dict):
        raise SystemExit("product evidence: tools/list returned no result")
    catalog = response["result"]
    tools = catalog.get("tools")
    if not isinstance(tools, list) or len(tools) != 13:
        raise SystemExit(f"product evidence: expected 13 tools, got {len(tools or [])}")
    return catalog


def load_candidate(
    path: pathlib.Path, brief: dict[str, object]
) -> tuple[dict[str, object], dict[str, object]]:
    candidate = json.loads(path.read_text(encoding="utf-8"))
    if candidate.get("contract") != CANDIDATE_CONTRACT:
        raise SystemExit("product evidence: candidate.json has the wrong contract")
    if candidate.get("source_sha") != brief["product_commit"]:
        raise SystemExit("product evidence: candidate source differs from campaign.json")
    expected_version = str(brief["binary"]["version"]).split()[1]
    if candidate.get("version") != expected_version:
        raise SystemExit("product evidence: candidate version differs from campaign.json")
    candidate_input = candidate.get("input_sha256")
    if not isinstance(candidate_input, str) or len(candidate_input) != 64 or any(
        character not in "0123456789abcdef" for character in candidate_input
    ):
        raise SystemExit("product evidence: candidate input_sha256 is invalid")
    source_sha = candidate.get("source_sha")
    if not isinstance(source_sha, str) or len(source_sha) != 40 or any(
        character not in "0123456789abcdef" for character in source_sha
    ):
        raise SystemExit("product evidence: candidate source_sha is not a full Git SHA")
    expected_input = run(
        [sys.executable, str(ROOT / "scripts" / "release" / "release-candidate.py"), "inputs"],
        capture_output=True,
        cwd=ROOT,
    ).stdout.strip().splitlines()[-1]
    if candidate_input != expected_input:
        raise SystemExit("product evidence: candidate input digest differs from release inputs")
    run_id = candidate.get("run_id")
    if not isinstance(run_id, str) or not run_id or run_id == "local":
        raise SystemExit("product evidence: candidate run_id is not a CI run")

    asset_name = pathlib.PurePosixPath(str(brief["binary"]["path"])).name
    records = [item for item in candidate.get("assets", []) if item.get("name") == asset_name]
    if len(records) != 1:
        raise SystemExit(f"product evidence: candidate does not uniquely bind {asset_name}")
    record = records[0]
    if record.get("sha256") != brief["binary"]["sha256"]:
        raise SystemExit("product evidence: candidate binary hash differs from campaign.json")
    if not isinstance(record.get("size"), int) or record["size"] <= 0:
        raise SystemExit("product evidence: candidate binary size is invalid")
    return candidate, record


def validate_binary(
    binary: pathlib.Path,
    brief: dict[str, object],
    record: dict[str, object],
) -> dict[str, object]:
    if not binary.is_file():
        raise SystemExit(f"product evidence: missing candidate binary {binary}")
    if binary.name != record["name"]:
        raise SystemExit("product evidence: local binary name differs from candidate asset")
    if binary.stat().st_size != record["size"] or sha256(binary) != record["sha256"]:
        raise SystemExit("product evidence: local binary differs from candidate.json")
    checksum = binary.with_name(f"{binary.name}.sha256")
    if checksum.is_file():
        fields = checksum.read_text(encoding="utf-8").split()
        if not fields or fields[0] != record["sha256"]:
            raise SystemExit("product evidence: candidate checksum sidecar is stale")
        if len(fields) > 1 and pathlib.PurePosixPath(fields[1]).name != binary.name:
            raise SystemExit("product evidence: candidate checksum names another asset")
    version = run([str(binary), "--version"], capture_output=True).stdout.strip()
    if version != brief["binary"]["version"]:
        raise SystemExit("product evidence: binary version differs from campaign.json")
    return tools_list(binary)


def static_expected(
    brief: dict[str, object],
    candidate: dict[str, object],
    catalog: dict[str, object],
) -> dict[pathlib.Path, str]:
    asset_name = pathlib.PurePosixPath(str(brief["binary"]["path"])).name
    actual_sha = str(brief["binary"]["sha256"])
    version = str(brief["binary"]["version"])
    commit = str(brief["product_commit"])
    release_version = version.split()[1]
    return {
        PRODUCT / "candidate.json": json.dumps(candidate, indent=2) + "\n",
        PRODUCT / "binary.sha256": f"{actual_sha}  {asset_name}\n",
        PRODUCT / "version.txt": f"{version}\n",
        PRODUCT / "tools-list.json": json.dumps(catalog, indent=2, sort_keys=True) + "\n",
        RELEASE / "commit.txt": f"{commit}\n",
        RELEASE / "tag.txt": f"v{release_version}\n",
    }


def read_catalog() -> dict[str, object]:
    path = PRODUCT / "tools-list.json"
    if not path.is_file():
        raise SystemExit("product evidence: product/tools-list.json is missing")
    catalog = json.loads(path.read_text(encoding="utf-8"))
    tools = catalog.get("tools")
    if not isinstance(tools, list) or len(tools) != 13:
        raise SystemExit("product evidence: stored tools/list does not contain thirteen tools")
    return catalog


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("write", "check"))
    parser.add_argument("--candidate", type=pathlib.Path)
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        help="downloaded candidate asset; optional live reprobe for check",
    )
    args = parser.parse_args()
    brief = json.loads((CAMPAIGN / "campaign.json").read_text(encoding="utf-8"))

    if args.command == "write":
        if args.candidate is None or args.binary is None:
            parser.error("write requires --candidate CANDIDATE.json and --binary CANDIDATE_ASSET")
        candidate, record = load_candidate(args.candidate.resolve(), brief)
        catalog = validate_binary(args.binary.resolve(), brief, record)
        values = static_expected(brief, candidate, catalog)
        for path, content in values.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        print("campaign product evidence: frozen")
        return

    candidate_path = PRODUCT / "candidate.json"
    if not candidate_path.is_file():
        raise SystemExit("product evidence: product/candidate.json is missing")
    candidate, record = load_candidate(candidate_path, brief)
    catalog = read_catalog()
    values = static_expected(brief, candidate, catalog)
    stale = [
        path.relative_to(ROOT).as_posix()
        for path, content in values.items()
        if not path.is_file() or path.read_text(encoding="utf-8") != content
    ]
    if stale:
        raise SystemExit(
            "campaign product evidence is missing or stale:\n"
            + "\n".join(f"- {path}" for path in stale)
        )
    binary = args.binary
    if binary is None and os.environ.get("KMP_MCP_BIN"):
        binary = pathlib.Path(os.environ["KMP_MCP_BIN"])
    if binary is not None and validate_binary(binary.resolve(), brief, record) != catalog:
        raise SystemExit("product evidence: live candidate tools/list differs from frozen evidence")
    print("campaign product evidence: current")


if __name__ == "__main__":
    main()
