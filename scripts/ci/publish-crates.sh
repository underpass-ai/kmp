#!/usr/bin/env bash
set -euo pipefail

# Publish the workspace's public crates to crates.io, in dependency order.
#
# Order is not a preference: cargo refuses to upload a crate whose
# requirements it cannot resolve on the registry, so a dependency that is
# not there yet fails the whole release. The list below is the transitive
# closure needed by `kmp-mcp`, deepest first.
#
# Dev-dependencies count. A dev-dependency with no version is dropped from
# the published manifest, but these share the workspace's pins with the
# normal dependencies, so they carry a version and cargo insists on
# resolving them — which is why `kmp-application` is published before
# `kmp-adapter-embedded`, whose only edge to it is a dev-dependency.
#
# Everything outside the chain — the server and its transport, the adapters
# it deploys with, the test crates — is marked `publish = false` in its own
# manifest.
#
# Two properties this script guarantees:
#
#   * Idempotence. A version already on the registry is skipped rather
#     than retried, because crates.io refuses a re-upload and a release
#     that half-published must be resumable by re-running the job.
#   * Patience. crates.io allows a burst of new crate names and then
#     throttles to one every ten minutes. A first release publishes far
#     more names than that burst allows, so a 429 is an expected state,
#     not a failure.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

CRATES=(
  kmp-plugin-api
  kmp-domain
  kmp-ports
  kmp-observability
  kmp-memory-api
  kmp-application
  kmp-adapter-embedded
  kmp-embedded
  kmp-proto
  kmp-proto-mapping
  kmp-viewer
  kmp-mcp
)

: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN must be set}"
: "${PUBLISH_MAX_WAIT_SECS:=1800}"
USER_AGENT="kmp-release (https://github.com/underpass-ai/kmp)"

# Before anything reaches the registry: the chain must still describe the
# workspace. crates.io versions are immutable, so a release that discovers
# a missing crate halfway cannot be undone, only worked around.
bash scripts/ci/check-publish-chain.sh

version_of() {
  cargo metadata --no-deps --format-version 1 \
    | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']==sys.argv[1]))" "$1"
}

already_published() {
  local crate="$1" version="$2" body
  body="$(curl -sS -H "User-Agent: ${USER_AGENT}" \
    "https://crates.io/api/v1/crates/${crate}/${version}" || true)"
  [[ "${body}" == *"\"num\":\"${version}\""* ]]
}

publish_one() {
  local crate="$1" version="$2" waited=0 delay=60 output status

  while :; do
    set +e
    # Verification stays on: it builds the packaged tarball, which is the
    # only thing that catches a file the crate forgot to ship — the way a
    # missing .proto and a missing fixture both look until someone
    # actually consumes the crate.
    output="$(cargo publish -p "${crate}" 2>&1)"
    status=$?
    set -e
    if [[ ${status} -eq 0 ]]; then
      echo "published ${crate} ${version}"
      return 0
    fi
    # Losing the race with our own earlier attempt is success, not failure.
    if grep -qi "already .*uploaded\|already exists" <<<"${output}"; then
      echo "${crate} ${version} was already on the registry"
      return 0
    fi
    if ! grep -qi "429\|too many requests\|rate limit" <<<"${output}"; then
      echo "${output}" >&2
      return 1
    fi
    if (( waited >= PUBLISH_MAX_WAIT_SECS )); then
      echo "::error::rate limited for ${waited}s publishing ${crate}; giving up" >&2
      echo "${output}" >&2
      return 1
    fi
    echo "::notice::crates.io rate limit hit on ${crate}; retrying in ${delay}s"
    sleep "${delay}"
    waited=$(( waited + delay ))
    delay=$(( delay < 600 ? delay * 2 : 600 ))
  done
}

for crate in "${CRATES[@]}"; do
  version="$(version_of "${crate}")"
  if already_published "${crate}" "${version}"; then
    echo "skip ${crate} ${version}: already on crates.io"
    continue
  fi
  echo "::group::cargo publish -p ${crate} (${version})"
  publish_one "${crate}" "${version}"
  echo "::endgroup::"
done

echo "crate publication complete"
