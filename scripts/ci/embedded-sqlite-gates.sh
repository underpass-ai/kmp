#!/usr/bin/env bash
# ADR-018: the SQLite engine behind the storage seam.
#
# Everything the default build proves about the embedded store, proved again
# with the opt-in engine compiled in — plus the two things only this engine
# claims: it survives kill -9 like redb does, and two processes can write the
# same store without losing an event. The default build's own gates stay
# untouched; this runs alongside them, never instead of them.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

echo "sqlite-gates: clippy with the engine compiled in"
cargo clippy -p kmp-adapter-embedded -p kmp-conformance -p kmp-embedded -p kmp-mcp \
  --all-targets --features sqlite --locked -- -D warnings

echo "sqlite-gates: the sixteen conformance scenarios on the sqlite engine"
cargo test -p kmp-conformance --features sqlite --locked --test embedded_sqlite_conformance

echo "sqlite-gates: adapter suite with the engine compiled in (redb arm must be unchanged)"
cargo test -p kmp-adapter-embedded --features sqlite --locked

echo "sqlite-gates: kmp-embedded and kmp-mcp still build and pass with the engine in"
cargo test -p kmp-embedded -p kmp-mcp --features sqlite --locked

echo "sqlite-gates: passed"
