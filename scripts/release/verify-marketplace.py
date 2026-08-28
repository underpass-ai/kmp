#!/usr/bin/env python3
"""Refuse a release whose public plugin marketplace is not version-aligned."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.parse
import urllib.request


DEFAULT_ROOT = "https://raw.githubusercontent.com/underpass-ai/plugins/main/plugins/kmp"
MANIFESTS = (
    ("Claude", ".claude-plugin/plugin.json"),
    ("Codex", ".codex-plugin/plugin.json"),
)


def read_manifest(root: str, relative: str) -> dict[str, object]:
    if root.startswith("https://"):
        url = urllib.parse.urljoin(f"{root.rstrip('/')}/", relative)
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "kmp-marketplace-release-gate/1"},
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read()
        except (OSError, urllib.error.URLError) as error:
            raise SystemExit(f"could not read public marketplace manifest {url}: {error}")
        source = url
    else:
        path = pathlib.Path(root) / relative
        try:
            payload = path.read_bytes()
        except OSError as error:
            raise SystemExit(f"could not read marketplace manifest {path}: {error}")
        source = str(path)

    try:
        value = json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"marketplace manifest {source} is not valid JSON: {error}")
    if not isinstance(value, dict):
        raise SystemExit(f"marketplace manifest {source} must contain a JSON object")
    return value


def verify(expected: str, root: str) -> list[tuple[str, str]]:
    observed: list[tuple[str, str]] = []
    for host, relative in MANIFESTS:
        manifest = read_manifest(root, relative)
        version = manifest.get("version")
        if not isinstance(version, str) or not version:
            raise SystemExit(f"{host} marketplace manifest has no string version")
        if version.split("+", 1)[0] != expected:
            raise SystemExit(
                f"kmp@underpass for {host} is {version!r}, not {expected!r}; "
                "merge the underpass-ai/plugins mirror PR before promoting the release"
            )
        observed.append((host, version))
    return observed


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", help="release SemVer without the v prefix")
    parser.add_argument(
        "--root",
        default=DEFAULT_ROOT,
        help="public raw plugin root, or a local fixture root for contract tests",
    )
    args = parser.parse_args()

    observed = verify(args.version, args.root)
    versions = ", ".join(f"{host}={version}" for host, version in observed)
    print(f"marketplace parity verified: kmp@underpass {versions}")


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(1)
