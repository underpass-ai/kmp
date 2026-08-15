#!/usr/bin/env bash

set -euo pipefail

# The release publishes a fixed list of crates, in dependency order
# (scripts/ci/publish-crates.sh). This checks that the list still describes
# the workspace:
#
#   * every crate that is publishable appears in the chain — a new crate
#     added without `publish = false` and without a place in the chain is a
#     release that fails halfway, after some crates are already on the
#     registry and immutable;
#   * every crate in the chain is publishable;
#   * the order respects dependencies — cargo cannot upload a crate whose
#     siblings are not on the registry yet;
#   * every published crate carries a version requirement on its internal
#     dependencies, since a path alone does not resolve on crates.io.

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

position = {name: i for i, name in enumerate(chain)}
for name in chain:
    package = packages.get(name)
    if package is None:
        problems.append(f"{name} is in the chain but not in the workspace")
        continue
    for dependency in package["dependencies"]:
        if dependency["kind"] == "dev":
            # Dev-dependencies without a version are stripped from the
            # published manifest, so they may point anywhere in the workspace.
            continue
        if dependency["name"] in packages and dependency["name"] not in publishable:
            problems.append(
                f"{name} depends on {dependency['name']}, which is publish = false"
            )
            continue
        if dependency["name"] not in publishable:
            continue
        if dependency["name"] not in position:
            continue
        if position[dependency["name"]] > position[name]:
            problems.append(
                f"{name} is published before its dependency {dependency['name']}"
            )
        if dependency["req"] == "*":
            problems.append(
                f"{name} depends on {dependency['name']} with no version requirement"
            )

if problems:
    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    sys.exit(1)

print(f"publish chain OK: {len(chain)} crates, in dependency order")
PY
