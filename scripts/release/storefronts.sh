#!/usr/bin/env bash
# Where every storefront stands for one KMP version, and whether they agree.
#
#   bash scripts/release/storefronts.sh            # the version this tree declares
#   bash scripts/release/storefronts.sh 0.25.0     # an explicit version
#
# A release is public when the GitHub release carries its assets, crates.io
# serves kmp-mcp, the MCP Registry lists the version as latest and the
# marketplace branch points at the tagged commit. Each of those is published
# by a different job, and a job that fails quietly leaves a storefront a
# release behind — which nobody notices until an install does. This prints
# one line per storefront and exits non-zero when any of them lags.
#
# Read-only: it publishes nothing. `publish-distribution.yml` resumes the
# crate chain with its `resume_crates` input; `mcp-registry.yml` publishes
# the Registry entry on dispatch when MCP_REGISTRY_PUBLISH is armed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"

version="${1:-}"
if [[ -z "${version}" ]]; then
  version="$(python3 -c 'import json; print(json.load(open("server.json"))["version"])')"
fi
version="${version#v}"
tag="v${version}"
agent="kmp-storefronts (https://github.com/underpass-ai/kmp)"

fetch() {
  curl --proto '=https' --tlsv1.2 -fsSL -A "${agent}" "$@"
}

status=0
report() {
  # report <storefront> <ok|lag|unknown> <detail>
  printf '%-14s %-8s %s\n' "$1" "$2" "$3"
  [[ "$2" == "ok" ]] || status=1
}

# GitHub release: present, with every checksummed asset.
if release="$(fetch "https://api.github.com/repos/underpass-ai/kmp/releases/tags/${tag}" 2>/dev/null)"; then
  assets="$(python3 -c 'import json,sys; r=json.load(sys.stdin); print(len(r.get("assets", [])))' <<<"${release}")"
  if [[ "${assets}" -ge 22 ]]; then
    report "github" "ok" "${tag} with ${assets} assets"
  else
    report "github" "lag" "${tag} exists with ${assets} assets; a release carries 22"
  fi
else
  report "github" "lag" "no release ${tag}"
fi

# crates.io: the version is on the registry and resolvable from the index.
if fetch "https://crates.io/api/v1/crates/kmp-mcp/${version}" >/dev/null 2>&1; then
  if fetch "https://index.crates.io/km/p-/kmp-mcp" 2>/dev/null | grep -q "\"vers\":\"${version}\""; then
    report "crates.io" "ok" "kmp-mcp ${version} published and in the index"
  else
    report "crates.io" "lag" "kmp-mcp ${version} published but not yet in the index"
  fi
else
  latest="$(fetch "https://crates.io/api/v1/crates/kmp-mcp" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["crate"]["max_version"])' 2>/dev/null || echo unknown)"
  report "crates.io" "lag" "kmp-mcp ${version} absent; newest is ${latest} (resume with publish-distribution.yml → resume_crates)"
fi

# MCP Registry: this version is listed and is the latest.
if versions="$(fetch "https://registry.modelcontextprotocol.io/v0/servers/io.github.underpass-ai%2Fkmp/versions" 2>/dev/null)"; then
  verdict="$(WANTED="${version}" VERSIONS="${versions}" python3 -c '
import json, os
wanted = os.environ["WANTED"]
servers = json.loads(os.environ["VERSIONS"]).get("servers", [])
mine = [s for s in servers if s["server"]["version"] == wanted]
latest = [s["server"]["version"] for s in servers
          if s.get("_meta", {}).get("io.modelcontextprotocol.registry/official", {}).get("isLatest")]
newest = latest[0] if latest else "unknown"
def key(version):
    return tuple(int(piece) for piece in version.split("+")[0].split("-")[0].split("."))
if not mine:
    print(f"lag {wanted} absent; latest is {newest} (mcp-registry.yml publishes on dispatch)")
elif wanted in latest:
    print(f"ok {wanted} is the latest listing")
elif newest != "unknown" and key(newest) > key(wanted):
    print(f"ok {wanted} listed; {newest} is newer")
else:
    print(f"lag {wanted} listed but latest is {newest}")
')"
  report "mcp-registry" "${verdict%% *}" "${verdict#* }"
else
  report "mcp-registry" "unknown" "the Registry did not answer"
fi

# Marketplace branch: Claude Code clones the tag it pins, from this branch.
if catalog="$(fetch "https://raw.githubusercontent.com/underpass-ai/kmp/marketplace/.claude-plugin/marketplace.json" 2>/dev/null)"; then
  pinned="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["plugins"][0]["source"]["ref"])' <<<"${catalog}")"
  if [[ "${pinned}" == "${tag}" ]]; then
    report "marketplace" "ok" "branch pins ${pinned}"
  elif [[ "$(printf '%s\n%s\n' "${pinned#v}" "${version}" | sort -V | tail -n1)" == "${pinned#v}" ]]; then
    report "marketplace" "ok" "branch pins ${pinned}, newer than ${tag}"
  else
    report "marketplace" "lag" "branch pins ${pinned}, not ${tag} (release.sh publish advances it)"
  fi
else
  report "marketplace" "unknown" "the marketplace branch did not answer"
fi

if [[ "${status}" -ne 0 ]]; then
  echo "storefronts: ${tag} is not everywhere it is advertised" >&2
fi
exit "${status}"
