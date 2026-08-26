#!/usr/bin/env python3
"""Assemble and verify the immutable asset set promoted by a release tag."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTRACT = "kmp.release-candidate.v1"
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
)
PLUGIN_LABELS = (
    "linux-x86_64",
    "linux-arm64",
    "macos-arm64",
    "windows-x86_64",
)


def tracked_inputs() -> list[pathlib.Path]:
    tracked = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.split(b"\0")
    exact = {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "LICENSE",
        "NOTICE",
        "THIRD_PARTY_NOTICES.md",
        ".github/workflows/release.yml",
        "scripts/ci/install-protoc.sh",
        "scripts/ci/install-rust-toolchain.sh",
    }
    prefixes = (
        "crates/",
        "api/",
        ".github/actions/install-rust/",
        "distribution/mcpb/",
        "plugins/kmp/",
        "scripts/plugin/",
        "scripts/release/",
    )
    selected = []
    for raw in tracked:
        if not raw:
            continue
        relative = raw.decode("utf-8")
        if relative in exact or relative.startswith(prefixes):
            selected.append(ROOT / relative)
    return sorted(selected, key=lambda path: path.relative_to(ROOT).as_posix())


def input_digest() -> str:
    digest = hashlib.sha256()
    for path in tracked_inputs():
        relative = path.relative_to(ROOT).as_posix().encode("utf-8")
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def expected_asset_names(version: str) -> list[str]:
    names: list[str] = []
    for target in TARGETS:
        suffix = ".exe" if target.endswith("windows-msvc") else ""
        name = f"kmp-mcp-v{version}-{target}{suffix}"
        names.extend((name, f"{name}.sha256"))
    mcpb = f"kmp-mcp-v{version}.mcpb"
    names.extend((mcpb, f"{mcpb}.sha256"))
    for label in PLUGIN_LABELS:
        name = f"kmp-plugin-{version}-{label}.tar.gz"
        names.extend((name, f"{name}.sha256"))
    return sorted(names)


def locate(name: str, roots: list[pathlib.Path]) -> pathlib.Path:
    matches = sorted({path.resolve() for root in roots for path in root.rglob(name) if path.is_file()})
    if len(matches) != 1:
        raise SystemExit(f"candidate expected exactly one {name}, found {len(matches)}")
    return matches[0]


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_checksum(asset: pathlib.Path, checksum: pathlib.Path) -> None:
    fields = checksum.read_text(encoding="utf-8").strip().split()
    if not fields or fields[0] != file_sha256(asset):
        raise SystemExit(f"candidate checksum does not match {asset.name}")


def assemble(args: argparse.Namespace) -> None:
    output = args.output.resolve()
    if output.exists():
        shutil.rmtree(output)
    assets = output / "assets"
    assets.mkdir(parents=True)
    roots = [args.binaries.resolve(), args.plugins.resolve(), args.mcpb.resolve()]
    names = expected_asset_names(args.version)
    for name in names:
        shutil.copy2(locate(name, roots), assets / name)

    records = []
    for name in names:
        path = assets / name
        if name.endswith(".sha256"):
            continue
        checksum = assets / f"{name}.sha256"
        validate_checksum(path, checksum)
        records.append({"name": name, "sha256": file_sha256(path), "size": path.stat().st_size})

    manifest = {
        "contract": CONTRACT,
        "version": args.version,
        "input_sha256": input_digest(),
        "source_sha": os.environ.get("GITHUB_SHA") or subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "source_ref": os.environ.get("GITHUB_REF_NAME", "local"),
        "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
        "assets": records,
    }
    (output / "candidate.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(
        f"release candidate assembled: {len(names)} files, "
        f"inputs {manifest['input_sha256']}, run {manifest['run_id']}"
    )


def verify(args: argparse.Namespace) -> None:
    directory = args.directory.resolve()
    manifest_path = directory / "candidate.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("contract") != CONTRACT:
        raise SystemExit(f"unexpected candidate contract: {manifest.get('contract')!r}")
    if manifest.get("version") != args.version:
        raise SystemExit(
            f"candidate version {manifest.get('version')!r} does not match {args.version!r}"
        )
    expected_input = args.input_sha256 or input_digest()
    if manifest.get("input_sha256") != expected_input:
        raise SystemExit(
            "candidate release inputs differ: "
            f"candidate={manifest.get('input_sha256')} current={expected_input}"
        )
    if args.run_id and str(manifest.get("run_id")) != str(args.run_id):
        raise SystemExit(
            f"candidate run {manifest.get('run_id')!r} does not match approved run {args.run_id!r}"
        )

    names = expected_asset_names(args.version)
    actual = sorted(path.name for path in (directory / "assets").iterdir() if path.is_file())
    if actual != names:
        missing = sorted(set(names) - set(actual))
        extra = sorted(set(actual) - set(names))
        raise SystemExit(f"candidate asset set differs: missing={missing}, extra={extra}")
    records = {record["name"]: record for record in manifest.get("assets", [])}
    for name in names:
        if name.endswith(".sha256"):
            continue
        asset = directory / "assets" / name
        checksum = directory / "assets" / f"{name}.sha256"
        validate_checksum(asset, checksum)
        record = records.get(name)
        if record is None or record.get("sha256") != file_sha256(asset) or record.get("size") != asset.stat().st_size:
            raise SystemExit(f"candidate manifest does not describe {name}")

    server = json.loads((ROOT / "server.json").read_text(encoding="utf-8"))
    declared_mcpb = next(
        package["fileSha256"]
        for package in server["packages"]
        if package.get("registryType") == "mcpb"
    )
    mcpb = directory / "assets" / f"kmp-mcp-v{args.version}.mcpb"
    if declared_mcpb != file_sha256(mcpb):
        raise SystemExit(
            f"server.json MCPB hash {declared_mcpb} does not match candidate {file_sha256(mcpb)}"
        )
    print(
        f"release candidate verified: version {args.version}, run {manifest['run_id']}, "
        f"20 files, inputs {expected_input}"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    inputs = subparsers.add_parser("inputs")
    inputs.add_argument("--github-output", type=pathlib.Path)

    assemble_parser = subparsers.add_parser("assemble")
    assemble_parser.add_argument("--version", required=True)
    assemble_parser.add_argument("--binaries", required=True, type=pathlib.Path)
    assemble_parser.add_argument("--plugins", required=True, type=pathlib.Path)
    assemble_parser.add_argument("--mcpb", required=True, type=pathlib.Path)
    assemble_parser.add_argument("--output", required=True, type=pathlib.Path)

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--version", required=True)
    verify_parser.add_argument("--directory", required=True, type=pathlib.Path)
    verify_parser.add_argument("--input-sha256")
    verify_parser.add_argument("--run-id")

    args = parser.parse_args()
    if args.command == "inputs":
        value = input_digest()
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as handle:
                print(f"input_sha256={value}", file=handle)
        print(value)
    elif args.command == "assemble":
        assemble(args)
    elif args.command == "verify":
        verify(args)
    else:
        raise AssertionError(args.command)


if __name__ == "__main__":
    main()
