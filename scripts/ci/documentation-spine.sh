#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from __future__ import annotations

import re
import sys
from collections import deque
from pathlib import Path

root = Path.cwd().resolve()
docs_root = root / "docs"
inventory_path = docs_root / "documentation-inventory.tsv"

rules: list[tuple[str, str, str]] = []
for line_number, line in enumerate(inventory_path.read_text(encoding="utf-8").splitlines(), 1):
    if not line or line.startswith("#"):
        continue
    fields = line.split("\t")
    if len(fields) != 3:
        sys.exit(f"{inventory_path}:{line_number}: expected status, glob and reason")
    status, pattern, reason = fields
    if status not in {"current", "research", "historical"}:
        sys.exit(f"{inventory_path}:{line_number}: invalid status {status!r}")
    rules.append((status, pattern, reason))

documents = sorted(path.relative_to(root).as_posix() for path in docs_root.rglob("*.md"))
classified: dict[str, str] = {}
for document in documents:
    matches = [(status, pattern) for status, pattern, _ in rules if Path(document).match(pattern)]
    if not matches:
        sys.exit(f"documentation inventory: unclassified document: {document}")
    if len(matches) > 1:
        sys.exit(f"documentation inventory: {document} matches more than one rule: {matches}")
    classified[document] = matches[0][0]

for status, pattern, _ in rules:
    if not any(Path(document).match(pattern) for document in documents):
        sys.exit(f"documentation inventory: rule matches nothing: {status}\t{pattern}")

link_pattern = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")

def markdown_links(path: Path) -> list[Path]:
    targets: list[Path] = []
    for raw in link_pattern.findall(path.read_text(encoding="utf-8", errors="replace")):
        raw = raw.split("#", 1)[0].strip().strip("<>")
        if not raw or "://" in raw or raw.startswith("mailto:"):
            continue
        target = (path.parent / raw).resolve()
        if target.is_dir():
            target = target / "README.md"
        if target.suffix.lower() != ".md" or not target.exists():
            continue
        if docs_root == target or docs_root in target.parents:
            targets.append(target)
    return targets

start = docs_root / "index.md"
distance: dict[Path, int] = {start: 0}
queue: deque[Path] = deque([start])
while queue:
    current = queue.popleft()
    if distance[current] >= 2:
        continue
    for target in markdown_links(current):
        next_distance = distance[current] + 1
        if target not in distance or next_distance < distance[target]:
            distance[target] = next_distance
            queue.append(target)

unreachable = [document for document in documents if (root / document) not in distance]
if unreachable:
    print("documentation spine: documents farther than two links from docs/index.md:", file=sys.stderr)
    for document in unreachable:
        print(f"  {document}", file=sys.stderr)
    sys.exit(1)

protocol = (root / "crates/kmp-mcp/src/protocol.rs").read_text(encoding="utf-8")
tool_names = set(
    re.findall(r'tool_definition(?:_with_output)?\(\s*"(kmp_[a-z0-9_]+)"', protocol)
)
if len(tool_names) != 10:
    sys.exit(f"documentation contract: expected ten tools in protocol.rs, found {sorted(tool_names)}")

current_paths = [root / path for path, status in classified.items() if status == "current"]
current_paths.extend([root / "README.md", root / "plugins/kmp/README.md"])
current_text = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in current_paths)

former_tools = sorted(set(re.findall(
    r"\bkernel_(?:ingest|write_memory|wake|ask|goto|near|rewind|forward|trace|inspect)\b",
    current_text,
)))
if former_tools:
    sys.exit(f"documentation contract: current docs use former public tool names: {former_tools}")

known_non_tools = {
    "kmp_abouts", "kmp_adapter", "kmp_adapter_embedded", "kmp_application",
    "kmp_domain", "kmp_interpretation", "kmp_mcp_tool", "kmp_move",
    "kmp_plugin_api", "kmp_ref_prefixes", "kmp_runner", "kmp_scope_ids",
    "kmp_scorecard", "kmp_testkit",
}
mentioned = set(re.findall(r"\bkmp_[a-z0-9_]+\b", current_text))
unknown = sorted(mentioned - tool_names - known_non_tools)
if unknown:
    sys.exit(f"documentation contract: unknown kmp_* identifiers in current docs: {unknown}")

number_words = {
    "zero": 0, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
    "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
}
for raw in re.findall(r"\b(\d+|zero|one|two|three|four|five|six|seven|eight|nine|ten) (?:MCP )?tools\b", current_text, re.I):
    count = int(raw) if raw.isdigit() else number_words[raw.lower()]
    if count != len(tool_names):
        sys.exit(f"documentation contract: docs say {raw} tools, binary source has {len(tool_names)}")

counts = {status: sum(value == status for value in classified.values()) for status in ("current", "research", "historical")}
print(
    "documentation spine: "
    f"{len(documents)} documents reachable in <=2 links; "
    f"{counts['current']} current, {counts['research']} research, {counts['historical']} historical; "
    f"{len(tool_names)} public tools"
)
PY
