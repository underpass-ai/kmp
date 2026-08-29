#!/usr/bin/env python3
"""Keep the shipped KMP guides aligned with one release and live binary."""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tomllib


DEFAULT_ROOT = pathlib.Path(__file__).resolve().parents[2]


def fail(message: str) -> None:
    raise SystemExit(f"release guide: {message}")


def workspace_version(root: pathlib.Path) -> str:
    with (root / "Cargo.toml").open("rb") as handle:
        return str(tomllib.load(handle)["workspace"]["package"]["version"])


def binary_version(binary: pathlib.Path) -> str:
    result = subprocess.run(
        [str(binary), "--version"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    fields = result.stdout.strip().split()
    if result.returncode != 0 or len(fields) < 2 or fields[0] != "kmp-mcp":
        fail(f"cannot read the KMP version from {binary}: {result.stderr.strip()}")
    return fields[1]


def first_json_line(path: pathlib.Path) -> dict[str, object]:
    try:
        with path.open(encoding="utf-8") as handle:
            return json.loads(handle.readline())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path}: {error}")


def check_release_envelope(root: pathlib.Path, version: str) -> None:
    actual_workspace = workspace_version(root)
    if actual_workspace != version:
        fail(f"workspace is {actual_workspace}, not {version}")

    plugin = root / "plugins/kmp"
    for relative in (".claude-plugin/plugin.json", ".codex-plugin/plugin.json"):
        manifest = json.loads((plugin / relative).read_text(encoding="utf-8"))
        if manifest.get("version") != version:
            fail(f"{relative} is {manifest.get('version')!r}, not {version!r}")

    editorial = json.loads((plugin / "guide/editorial.json").read_text(encoding="utf-8"))
    guide_version = str(editorial.get("guide_version", ""))
    if not guide_version:
        fail("editorial.json has no guide_version")

    requests = json.loads(
        (plugin / "guide/guide.requests.json").read_text(encoding="utf-8")
    )
    if not isinstance(requests, list) or {
        request.get("about") for request in requests if isinstance(request, dict)
    } != {"guide:kmp", "guide:kmp-agent"}:
        fail("guide.requests.json does not contain exactly the human and agent guides")
    for request in requests:
        key = str(request.get("idempotency_key", ""))
        if f":{guide_version}:" not in key:
            fail(f"{request.get('about')} does not use editorial guide version {guide_version}")
        entries = request.get("memory", {}).get("entries", [])
        if not entries or any(
            str(entry.get("metadata", {}).get("guide_version")) != guide_version
            for entry in entries
        ):
            fail(f"{request.get('about')} entry metadata is stale")

    header = first_json_line(plugin / "guide/memory.jsonl")
    if header.get("bundle_format") != 2:
        fail("guide memory is not a format-2 bundle")
    if header.get("event_count") != 2:
        fail(f"guide bundle has {header.get('event_count')!r} events, expected 2")
    if header.get("kernel_version") != version:
        fail(
            "guide bundle targets kernel "
            f"{header.get('kernel_version')!r}, not release {version!r}; "
            f"run 'bash scripts/release.sh version {version}'"
        )
    if header.get("abouts") != ["guide:kmp", "guide:kmp-agent"]:
        fail(f"guide bundle has unexpected abouts: {header.get('abouts')!r}")


def sync(root: pathlib.Path, version: str, binary: pathlib.Path) -> None:
    resolved = binary.resolve()
    if not resolved.is_file():
        fail(f"binary does not exist: {resolved}")
    actual_binary = binary_version(resolved)
    if actual_binary != version:
        fail(f"binary is {actual_binary}, not {version}")
    builder = root / "plugins/kmp/guide/build-guide.py"
    try:
        check_release_envelope(root, version)
    except SystemExit:
        pass
    else:
        current = subprocess.run(
            [sys.executable, str(builder), "check", "--binary", str(resolved)],
            cwd=root,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if current.returncode == 0:
            print("release guide: shipped assets already match this exact binary")
            return
    subprocess.run(
        [
            sys.executable,
            str(builder),
            "write",
            "--binary",
            str(resolved),
        ],
        cwd=root,
        check=True,
    )
    check_release_envelope(root, version)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("sync", "check"))
    parser.add_argument("version")
    parser.add_argument("--binary", type=pathlib.Path)
    parser.add_argument("--root", type=pathlib.Path, default=DEFAULT_ROOT)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    root = args.root.resolve()
    if args.command == "sync":
        if args.binary is None:
            fail("sync requires --binary")
        sync(root, args.version, args.binary)
    else:
        if args.binary is not None:
            fail("check does not accept --binary")
        check_release_envelope(root, args.version)
    print(
        f"release guide: human guide, agent guide, plugin and engine agree on {args.version}"
    )


if __name__ == "__main__":
    main()
