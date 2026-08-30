#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# The architecture gate for #404.
#
# The migration is long, so the gate is a ratchet rather than a pass/fail wall:
# it accounts for every tracked source below crates/kmp-mcp/src/**, records
# today's violations in a checked-in baseline, and fails when the debt grows —
# a new file with two primary types, a monolith that gets longer, a file nobody
# declared. Paying debt down is always allowed and the gate says how much is
# left.
#
# Refresh the baseline deliberately, never to make a red build green:
#   KMP_ARCHITECTURE_BASELINE=write bash scripts/ci/kmp-mcp-architecture-gate.sh

python3 - <<'PY'
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

root = Path.cwd().resolve()
src = root / "crates/kmp-mcp/src"
baseline_path = root / "docs/architecture/kmp-mcp-conformance.tsv"

# A file this long is doing more than one thing, whatever its name says.
MONOLITH_LINES = 600

PRIMARY_TYPE = re.compile(
    r"^pub(?:\([^)]*\))?\s+(?:struct|enum|trait|union)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
TODO = "TODO(#404)"


def tracked_sources() -> list[Path]:
    listed = subprocess.run(
        ["git", "ls-files", "crates/kmp-mcp/src"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    tracked = {root / line for line in listed if line.endswith(".rs")}
    # Templates written by kmp-mcp-slice-templates.py are not committed until
    # they carry code, but the gate must still see them as declared work.
    return sorted(tracked | set(src.rglob("*.rs")))


def measure(path: Path) -> tuple[int, list[str], bool]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    types = [
        match.group(1)
        for line in lines
        if (match := PRIMARY_TYPE.match(line)) is not None
    ]
    return len(lines), types, TODO in text


def relative(path: Path) -> str:
    return path.relative_to(root).as_posix()


measured = {relative(path): measure(path) for path in tracked_sources()}

violations: dict[str, str] = {}
pending = 0
for name, (lines, types, todo) in sorted(measured.items()):
    if todo:
        pending += 1
        continue
    reasons = []
    if len(types) > 1:
        reasons.append(f"types={len(types)}")
    if lines > MONOLITH_LINES:
        reasons.append(f"lines={lines}")
    if reasons:
        violations[name] = ",".join(reasons)

if os.environ.get("KMP_ARCHITECTURE_BASELINE") == "write":
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    with baseline_path.open("w", encoding="utf-8") as handle:
        handle.write("# Architecture debt below crates/kmp-mcp/src, for #404.\n")
        handle.write(f"# A file is over budget at more than one primary public type or {MONOLITH_LINES} lines.\n")
        handle.write("# This baseline may shrink freely. It may only grow through a reviewed change.\n")
        handle.write("path\tdebt\n")
        for name, debt in sorted(violations.items()):
            handle.write(f"{name}\t{debt}\n")
    print(f"wrote {baseline_path.relative_to(root)} with {len(violations)} entries")
    sys.exit(0)

if not baseline_path.exists():
    sys.exit(
        f"missing {baseline_path.relative_to(root)}; write it with "
        "KMP_ARCHITECTURE_BASELINE=write bash scripts/ci/kmp-mcp-architecture-gate.sh"
    )

baseline: dict[str, str] = {}
for line in baseline_path.read_text(encoding="utf-8").splitlines():
    if not line or line.startswith("#") or line.startswith("path\t"):
        continue
    name, _, debt = line.partition("\t")
    baseline[name] = debt

failures: list[str] = []

for name, debt in sorted(violations.items()):
    if name not in baseline:
        failures.append(
            f"{name}: new architecture debt ({debt}). Split it, or add it to the "
            "baseline in a reviewed change that says why."
        )
        continue
    was = dict(part.split("=") for part in baseline[name].split(",") if "=" in part)
    now = dict(part.split("=") for part in debt.split(",") if "=" in part)
    for measure_name, value in now.items():
        if int(value) > int(was.get(measure_name, 0)):
            failures.append(
                f"{name}: {measure_name} grew from {was.get(measure_name, 0)} to {value}. "
                "A file already carrying debt must not take on more."
            )

paid = sorted(set(baseline) - set(violations))

print(f"kmp-mcp architecture gate: {len(measured)} tracked sources")
print(f"  debt carried:  {len(violations)} of {len(baseline)} baselined files")
print(f"  templates open: {pending} awaiting migration")
if paid:
    print(f"  paid down:     {len(paid)}")
    for name in paid:
        print(f"    {name}")
    print("  refresh the baseline with KMP_ARCHITECTURE_BASELINE=write")

if failures:
    print()
    for failure in failures:
        print(f"  {failure}")
    sys.exit(1)
PY
