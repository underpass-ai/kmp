#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${1:?usage: package-kmp-mcpb.sh VERSION INPUT_DIR [OUTPUT_DIR]}"
INPUT_DIR="${2:?usage: package-kmp-mcpb.sh VERSION INPUT_DIR [OUTPUT_DIR]}"
OUTPUT_DIR="${3:-${ROOT_DIR}/dist/mcpb}"
TAG="v${VERSION#v}"
VERSION="${VERSION#v}"

[[ "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || {
  echo "package-kmp-mcpb: invalid version '${VERSION}'" >&2
  exit 2
}

required=(
  "kmp-mcp-${TAG}-x86_64-unknown-linux-gnu"
  "kmp-mcp-${TAG}-aarch64-unknown-linux-gnu"
  "kmp-mcp-${TAG}-x86_64-apple-darwin"
  "kmp-mcp-${TAG}-aarch64-apple-darwin"
  "kmp-mcp-${TAG}-x86_64-pc-windows-msvc.exe"
)
for binary in "${required[@]}"; do
  [ -f "${INPUT_DIR}/${binary}" ] || {
    echo "package-kmp-mcpb: missing ${INPUT_DIR}/${binary}" >&2
    exit 1
  }
done

mkdir -p "${ROOT_DIR}/tmp"
work="$(mktemp -d "${ROOT_DIR}/tmp/mcpb-package.XXXXXX")"
trap 'rm -rf "${work}"' EXIT
stage="${work}/kmp"
mkdir -p "${stage}/server/bin" "${OUTPUT_DIR}"

python3 - "${ROOT_DIR}/distribution/mcpb/manifest.json" "${stage}/manifest.json" "${VERSION}" <<'PY'
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
version = sys.argv[3]
manifest = json.loads(source.read_text(encoding="utf-8"))
if manifest["version"] != version:
    raise SystemExit(
        f"manifest version {manifest['version']} does not match requested {version}"
    )
destination.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

install -m 755 "${ROOT_DIR}/distribution/mcpb/server/kmp-mcp" \
  "${stage}/server/kmp-mcp"
for binary in "${required[@]}"; do
  destination="${binary#kmp-mcp-${TAG}-}"
  install -m 755 "${INPUT_DIR}/${binary}" "${stage}/server/bin/kmp-mcp-${destination}"
done
cp "${stage}/server/bin/kmp-mcp-x86_64-pc-windows-msvc.exe" \
  "${stage}/server/kmp-mcp.exe"

archive="${OUTPUT_DIR}/kmp-mcp-${TAG}.mcpb"
python3 - "${stage}" "${archive}" <<'PY'
import pathlib
import sys
import zipfile

source = pathlib.Path(sys.argv[1])
archive = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
    for path in sorted(p for p in source.rglob("*") if p.is_file()):
        relative = path.relative_to(source).as_posix()
        info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
        info.create_system = 3
        mode = 0o755 if relative.startswith("server/") else 0o644
        info.external_attr = mode << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        with path.open("rb") as handle:
            bundle.writestr(info, handle.read(), compresslevel=9)
PY

if command -v sha256sum >/dev/null 2>&1; then
  (cd "${OUTPUT_DIR}" && sha256sum "$(basename "${archive}")" > "$(basename "${archive}").sha256")
else
  (cd "${OUTPUT_DIR}" && shasum -a 256 "$(basename "${archive}")" > "$(basename "${archive}").sha256")
fi

echo "MCPB ready at ${archive}"
