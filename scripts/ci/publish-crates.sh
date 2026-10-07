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
  kmp-memory-api
  kmp-application
  kmp-observability
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

# The path of a crate in the sparse index: 1/a, 2/ab, 3/a/abc, ab/cd/name.
index_path() {
  local crate="$1"
  case "${#crate}" in
    1) echo "1/${crate}" ;;
    2) echo "2/${crate}" ;;
    3) echo "3/${crate:0:1}/${crate}" ;;
    *) echo "${crate:0:2}/${crate:2:2}/${crate}" ;;
  esac
}

# Whether `cargo` would resolve this version right now. The API answers as
# soon as the upload is accepted; the index cargo reads lags it by a few
# minutes, and a dependent published in that window fails with "candidate
# versions found which didn't match" — which is how 0.25.0 left kmp-mcp off
# the registry while every crate beneath it was there.
visible_in_index() {
  local crate="$1" version="$2"
  curl -sS -H "User-Agent: ${USER_AGENT}" \
    "https://index.crates.io/$(index_path "${crate}")" 2>/dev/null \
    | grep -q "\"vers\":\"${version}\""
}

# Block until the index serves `version`, so the next crate in the chain can
# depend on it. Bounded like the rate-limit wait: an index that never catches
# up is a registry problem to report, not a loop to live in.
await_index() {
  local crate="$1" version="$2" waited=0 delay=10
  until visible_in_index "${crate}" "${version}"; do
    if (( waited >= PUBLISH_MAX_WAIT_SECS )); then
      echo "::error::${crate} ${version} was accepted but did not reach the index in ${waited}s" >&2
      return 1
    fi
    echo "waiting for ${crate} ${version} to reach the crates.io index (${waited}s)"
    sleep "${delay}"
    waited=$(( waited + delay ))
    delay=$(( delay < 60 ? delay * 2 : 60 ))
  done
  echo "${crate} ${version} is in the index"
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
  else
    echo "::group::cargo publish -p ${crate} (${version})"
    publish_one "${crate}" "${version}"
    echo "::endgroup::"
  fi
  # Even a crate that was already there is waited for: a resumed run may
  # start seconds after the upload that stopped the previous one.
  await_index "${crate}" "${version}"
done

echo "crate publication complete"
