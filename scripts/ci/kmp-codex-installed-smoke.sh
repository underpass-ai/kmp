#!/usr/bin/env bash
set -euo pipefail

# Exercise Codex's real plugin ingestion in an isolated home. The ordinary
# marketplace-shaped smoke checks the package; this one checks what Codex
# actually materializes after conversion.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
command -v codex >/dev/null 2>&1 || {
  echo "KMP installed Codex smoke skipped: codex is not available"
  exit 0
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
MARKET="$WORK/marketplace"
HOME_DIR="$WORK/home"
mkdir -p "$MARKET/.agents/plugins" "$MARKET/plugins/kmp" "$HOME_DIR"

while IFS= read -r tracked; do
  destination="$MARKET/plugins/kmp/${tracked#plugins/kmp/}"
  mkdir -p "$(dirname "$destination")"
  cp -p "$ROOT/$tracked" "$destination"
done < <(git -C "$ROOT" ls-files plugins/kmp)

python3 - "$MARKET/.agents/plugins/marketplace.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
body = {
    "name": "kmp-contract-test",
    "interface": {"displayName": "KMP Contract Test"},
    "plugins": [{
        "name": "kmp",
        "source": {"source": "local", "path": "./plugins/kmp"},
        "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
        "category": "Developer Tools",
    }],
}
path.write_text(json.dumps(body, indent=2) + "\n", encoding="utf-8")
PY

HOME="$HOME_DIR" codex plugin marketplace add "$MARKET" >/dev/null
HOME="$HOME_DIR" codex plugin add kmp@kmp-contract-test >/dev/null

CACHE="$(find "$HOME_DIR/.codex/plugins/cache/kmp-contract-test/kmp" -mindepth 1 -maxdepth 1 -type d | head -1)"
[ -n "$CACHE" ] || { echo "KMP installed Codex smoke: no installed cache" >&2; exit 1; }

python3 - "$ROOT/plugins/kmp/capabilities.json" "$CACHE" <<'PY'
import json
import pathlib
import sys

contract = json.loads(pathlib.Path(sys.argv[1]).read_text())
cache = pathlib.Path(sys.argv[2])
expected = {"kmp-memory", *(entry["codex_skill"] for entry in contract["human_workflows"])}
actual = {path.parent.name for path in (cache / "skills").glob("*/SKILL.md")}
if actual != expected:
    raise SystemExit(f"effective native skills differ: expected={sorted(expected)}, actual={sorted(actual)}")
migrated = cache / ".codex-plugin/migrated-command-skills"
if migrated.exists() and any(migrated.glob("*/SKILL.md")):
    names = sorted(path.parent.name for path in migrated.glob("*/SKILL.md"))
    raise SystemExit(f"Codex created an accidental second workflow surface: {names}")
print(f"KMP installed Codex smoke passed: {len(actual)} native skills, no migrated commands")
PY
