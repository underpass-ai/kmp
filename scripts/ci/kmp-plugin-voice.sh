#!/usr/bin/env bash
# One product, one voice — checked, not hoped for.
#
# plugins/kmp/VOICE.md is the source of truth. Every command in both hosts
# carries its block verbatim, and this fails the build when one drifts. A
# standard nobody enforces lasts until the next command is written in a hurry.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN="$ROOT/plugins/kmp"
VOICE="$PLUGIN/VOICE.md"
OPEN='<!-- kmp:voice -->'
CLOSE='<!-- /kmp:voice -->'

FAILURES=0
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILURES=$((FAILURES + 1)); }
ok()   { printf 'ok    %s\n' "$1"; }

[ -f "$VOICE" ] || { fail "$VOICE is missing — the voice has no source of truth"; exit 1; }

# The canonical block: everything between the markers in VOICE.md.
CANON="$(awk -v o="$OPEN" -v c="$CLOSE" '
  $0 == o { inside = 1; next }
  $0 == c { inside = 0; next }
  inside  { print }
' "$VOICE")"
[ -n "$CANON" ] || { fail "VOICE.md has no block between its markers"; exit 1; }

# Both hosts, every command. A file that opts out is drift with extra steps.
mapfile -t COMMANDS < <(
  find "$PLUGIN/commands" "$PLUGIN/codex/prompts" -name '*.md' -type f | sort
)
[ "${#COMMANDS[@]}" -gt 0 ] || { fail "no commands found to check"; exit 1; }

for file in "${COMMANDS[@]}"; do
  rel="${file#"$ROOT"/}"

  if ! grep -qF "$OPEN" "$file" || ! grep -qF "$CLOSE" "$file"; then
    fail "$rel has no voice block — copy the one at the bottom of VOICE.md"
    continue
  fi

  found="$(awk -v o="$OPEN" -v c="$CLOSE" '
    $0 == o { inside = 1; next }
    $0 == c { inside = 0; next }
    inside  { print }
  ' "$file")"

  if [ "$found" != "$CANON" ]; then
    fail "$rel drifted from VOICE.md — the block must match byte for byte"
    diff <(printf '%s\n' "$CANON") <(printf '%s\n' "$found") | sed 's/^/      /' >&2
    continue
  fi

  # Claude Code reads the description out of frontmatter; without it the
  # command exists and is invisible in the picker.
  case "$rel" in
    */commands/*)
      head -1 "$file" | grep -q '^---$' \
        || { fail "$rel has no frontmatter"; continue; }
      grep -q '^description: ' "$file" \
        || { fail "$rel has no description: line"; continue; }
      ;;
  esac

  ok "$rel"
done

# The two hosts offer the same commands or one of them is quietly poorer.
claude_names="$(find "$PLUGIN/commands" -name '*.md' -exec basename {} .md \; | sort | tr '\n' ' ')"
codex_names="$(find "$PLUGIN/codex/prompts" -name 'kmp-*.md' -exec basename {} .md \; | sed 's/^kmp-//' | sort | tr '\n' ' ')"
if [ "$claude_names" != "$codex_names" ]; then
  fail "the hosts do not offer the same commands"
  printf '      claude: %s\n      codex:  %s\n' "$claude_names" "$codex_names" >&2
else
  ok "both hosts offer the same commands: $claude_names"
fi

printf '\n'
if [ "$FAILURES" -gt 0 ]; then
  printf 'One voice, %d exception(s). Fix the FAIL lines above.\n' "$FAILURES" >&2
  exit 1
fi
printf 'One voice, no exceptions.\n'
