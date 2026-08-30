#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

python3 - <<'PY'
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

root = Path.cwd()
workspace_text = (root / "Cargo.toml").read_text(encoding="utf-8")
match = re.search(r'^version = "([^"]+)"', workspace_text, re.MULTILINE)
if not match:
    sys.exit("MCP Registry gate: workspace version is missing")
version = match.group(1)

server = json.loads((root / "server.json").read_text(encoding="utf-8"))
manifest = json.loads((root / "distribution/mcpb/manifest.json").read_text(encoding="utf-8"))

expected_name = "io.github.underpass-ai/kmp"
expected_schema = "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json"
if server.get("$schema") != expected_schema:
    sys.exit(f"MCP Registry gate: schema is not pinned to {expected_schema}")
if server.get("name") != expected_name:
    sys.exit(f"MCP Registry gate: namespace must remain {expected_name}")
if server.get("version") != version:
    sys.exit(f"MCP Registry gate: server.json says {server.get('version')}, workspace says {version}")
if manifest.get("version") != version:
    sys.exit(f"MCP Registry gate: MCPB manifest says {manifest.get('version')}, workspace says {version}")

packages = server.get("packages", [])
by_type: dict[str, list[dict]] = {}
for package in packages:
    by_type.setdefault(package.get("registryType", ""), []).append(package)
if set(by_type) != {"cargo", "mcpb"} or any(len(items) != 1 for items in by_type.values()):
    sys.exit("MCP Registry gate: server.json must expose exactly one Cargo and one MCPB package")

cargo = by_type["cargo"][0]
if cargo.get("identifier") != "kmp-mcp" or cargo.get("version") != version:
    sys.exit("MCP Registry gate: Cargo package name/version drifted")
if "runtimeHint" in cargo:
    sys.exit("MCP Registry gate: Cargo packages are installed once and must not declare runtimeHint")

mcpb = by_type["mcpb"][0]
expected_url = (
    f"https://github.com/underpass-ai/kmp/releases/download/v{version}/"
    f"kmp-mcp-v{version}.mcpb"
)
if mcpb.get("identifier") != expected_url:
    sys.exit(f"MCP Registry gate: MCPB URL must be {expected_url}")
sha256 = mcpb.get("fileSha256", "")
if not re.fullmatch(r"[a-f0-9]{64}", sha256) or sha256 == "0" * 64:
    sys.exit("MCP Registry gate: MCPB hash is missing or still the pre-build placeholder")

for package_type, package in (("Cargo", cargo), ("MCPB", mcpb)):
    if package.get("transport") != {"type": "stdio"}:
        sys.exit(f"MCP Registry gate: {package_type} must use stdio")
    variables = {item.get("name") for item in package.get("environmentVariables", [])}
    expected_variables = {
        "KMP_MCP_DATA_DIR",
        "KMP_VIEWER_ADDR",
        "KMP_KERNEL_GRPC_ENDPOINT",
    }
    if variables != expected_variables:
        sys.exit(
            f"MCP Registry gate: {package_type} environment variables are "
            f"{sorted(variables)}, expected {sorted(expected_variables)}"
        )

readme = (root / "crates/kmp-mcp/README.md").read_text(encoding="utf-8")
marker = f"mcp-name: {expected_name}"
visible = [line for line in readme.splitlines() if marker in line and "<!--" not in line]
if len(visible) != 1:
    sys.exit("MCP Registry gate: crate README needs one visible, exact mcp-name marker")

# The reviewed surface, not the source that builds it: `tools_list.json` is
# pinned against the running binary by `tool_surface_parity`, so a manifest
# checked against it is checked against what a host is actually served.
surface = json.loads(
    (root / "crates/kmp-mcp/fixtures/contract/tools_list.json").read_text(encoding="utf-8")
)
tool_names = {tool["name"] for tool in surface["tools"]}
manifest_tools = {tool.get("name") for tool in manifest.get("tools", [])}
if manifest_tools != tool_names:
    sys.exit(
        "MCP Registry gate: MCPB tool list drifted: "
        f"manifest={sorted(manifest_tools)}, surface={sorted(tool_names)}"
    )

print(
    f"MCP Registry contract passed: {expected_name} {version}, "
    f"Cargo + MCPB, {len(tool_names)} tools"
)
PY

if command -v mcp-publisher >/dev/null 2>&1; then
  mkdir -p "${ROOT_DIR}/tmp"
  publisher_home="$(mktemp -d "${ROOT_DIR}/tmp/mcp-publisher-validate.XXXXXX")"
  trap 'rm -rf "${publisher_home}"' EXIT
  mkdir -p "${publisher_home}/.config/mcp-publisher"
  printf '%s\n' \
    '{"token":"","method":"none","registry":"https://staging.registry.modelcontextprotocol.io"}' \
    > "${publisher_home}/.config/mcp-publisher/token.json"
  HOME="${publisher_home}" mcp-publisher validate server.json
fi
