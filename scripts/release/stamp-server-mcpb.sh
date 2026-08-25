#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCHIVE="${1:?usage: stamp-server-mcpb.sh PATH/TO/kmp-mcp-vX.Y.Z.mcpb}"
SERVER_JSON="${2:-${ROOT_DIR}/server.json}"
VERSION="$(
  python3 -c \
    'import tomllib, sys; print(tomllib.load(open(sys.argv[1], "rb"))["workspace"]["package"]["version"])' \
    "${ROOT_DIR}/Cargo.toml"
)"
EXPECTED="kmp-mcp-v${VERSION}.mcpb"

[ "$(basename "${ARCHIVE}")" = "${EXPECTED}" ] || {
  echo "stamp-server-mcpb: expected ${EXPECTED}, got $(basename "${ARCHIVE}")" >&2
  exit 1
}
[ -f "${ARCHIVE}" ] || {
  echo "stamp-server-mcpb: archive not found: ${ARCHIVE}" >&2
  exit 1
}

if command -v sha256sum >/dev/null 2>&1; then
  SHA256="$(sha256sum "${ARCHIVE}" | awk '{print $1}')"
else
  SHA256="$(shasum -a 256 "${ARCHIVE}" | awk '{print $1}')"
fi

python3 - "${SERVER_JSON}" "${VERSION}" "${SHA256}" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
version = sys.argv[2]
sha256 = sys.argv[3]
body = json.loads(path.read_text(encoding="utf-8"))

if body.get("version") != version:
    raise SystemExit(
        f"server.json version {body.get('version')} does not match workspace {version}"
    )

mcpb = [package for package in body.get("packages", []) if package.get("registryType") == "mcpb"]
if len(mcpb) != 1:
    raise SystemExit(f"server.json must contain exactly one MCPB package, found {len(mcpb)}")

mcpb[0]["identifier"] = (
    f"https://github.com/underpass-ai/kmp/releases/download/v{version}/"
    f"kmp-mcp-v{version}.mcpb"
)
mcpb[0]["fileSha256"] = sha256
path.write_text(json.dumps(body, indent=2) + "\n", encoding="utf-8")
print(f"stamped server.json with {mcpb[0]['identifier']} ({sha256})")
PY
