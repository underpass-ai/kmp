#!/usr/bin/env bash

set -euo pipefail

# The release publishes a fixed list of crates, in dependency order
# (scripts/ci/publish-crates.sh). This checks that the list still describes
# the workspace, on pull requests, where a mistake is still free —
# crates.io versions are immutable, and a release that discovers a bad
# order halfway leaves some crates published and the rest not.
#
# The check is a simulation, not a rule of thumb: walk the chain in order,
# carrying the set of crates already on the registry, and require that
# every internal dependency carrying a version requirement is in that set
# by the time its dependent is published. That is exactly what cargo does,
# and it is deliberately blind to dependency kind — a dev-dependency is
# only dropped from the published manifest when it has no version, and the
# ones here inherit the workspace's pins.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

python3 - scripts/ci/publish-crates.sh <<'PY'
import json
import re
import subprocess
import sys

metadata = json.loads(
    subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
)
packages = {p["name"]: p for p in metadata["packages"]}

# cargo reports `publish: []` for `publish = false`, null for publishable.
publishable = {name for name, p in packages.items() if p.get("publish") != []}

script = open(sys.argv[1]).read()
chain_block = re.search(r"^CRATES=\(\n(.*?)^\)", script, re.M | re.S)
if not chain_block:
    sys.exit("publish-crates.sh: could not read the CRATES list")
chain = chain_block.group(1).split()

problems = []

missing = publishable - set(chain)
if missing:
    problems.append(
        "publishable but not in the chain: "
        + ", ".join(sorted(missing))
        + " (add it to publish-crates.sh, or set publish = false)"
    )

extra = set(chain) - publishable
if extra:
    problems.append("in the chain but not publishable: " + ", ".join(sorted(extra)))

on_registry = set()
for name in chain:
    package = packages.get(name)
    if package is None:
        problems.append(f"{name} is in the chain but not in the workspace")
        continue
    for dependency in package["dependencies"]:
        if dependency["name"] not in packages:
            continue  # external, already on the registry
        if dependency["name"] not in publishable:
            if dependency["kind"] != "dev" or dependency["req"] != "*":
                problems.append(
                    f"{name} depends on {dependency['name']}, which is publish = false"
                )
            continue
        if dependency["req"] == "*":
            if dependency["kind"] == "dev":
                # Version-less dev-dependencies are stripped on publish.
                continue
            problems.append(
                f"{name} depends on {dependency['name']} with no version requirement"
            )
            continue
        if dependency["name"] not in on_registry:
            kind = dependency["kind"] or "normal"
            problems.append(
                f"{name} is published before its {kind} dependency {dependency['name']}"
            )
    on_registry.add(name)

if problems:
    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    sys.exit(1)

print(f"publish chain OK: {len(chain)} crates, in dependency order")
PY
