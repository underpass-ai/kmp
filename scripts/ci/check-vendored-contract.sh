#!/usr/bin/env bash

set -euo pipefail

# Two pieces of the contract live twice, and must stay one contract.
#
#   api/proto/...                     authored, linted, breaking-checked
#   crates/kmp-proto/proto/...        vendored copy compiled by the crate
#
#   api/examples/kernel/v1beta1/kmp/  reference request/response examples
#   crates/kmp-mcp/fixtures/...       vendored copy embedded in the binary
#
# The copies are not convenience. `cargo publish` packages only what lives
# inside the crate directory, so a crate that reads `../../api` builds in
# this repo and fails for everyone who installs it from crates.io. What the
# duplication costs is drift: the published adapter would answer with
# examples the kernel no longer produces, or speak a wire it no longer
# serves, and nothing would say so until it failed in someone else's
# process. This gate is what makes the copies safe.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

status=0

PROTO_CANONICAL="api/proto/underpass/rehydration/kernel/v1beta1"
PROTO_VENDORED="crates/kmp-proto/proto/underpass/rehydration/kernel/v1beta1"

if ! diff -ru "${PROTO_CANONICAL}" "${PROTO_VENDORED}"; then
  echo "::error::${PROTO_VENDORED} has drifted from ${PROTO_CANONICAL}" >&2
  echo "fix: cp ${PROTO_CANONICAL}/*.proto ${PROTO_VENDORED}/" >&2
  status=1
fi

FIXTURE_CANONICAL="api/examples/kernel/v1beta1/kmp"
FIXTURE_VENDORED="crates/kmp-mcp/fixtures/kernel/v1beta1/kmp"

# Compared file by file, not directory to directory: the crate embeds only
# the responses its fixture backend answers with, and the canonical
# directory legitimately holds more than that (requests, schema, README).
for vendored in "${FIXTURE_VENDORED}"/*.json; do
  canonical="${FIXTURE_CANONICAL}/$(basename "${vendored}")"
  if [[ ! -f "${canonical}" ]]; then
    echo "::error::${vendored} has no counterpart at ${canonical}" >&2
    status=1
    continue
  fi
  if ! diff -u "${canonical}" "${vendored}"; then
    echo "::error::${vendored} has drifted from ${canonical}" >&2
    echo "fix: cp ${canonical} ${vendored}" >&2
    status=1
  fi
done

if [[ ${status} -ne 0 ]]; then
  exit 1
fi

echo "vendored proto and fixtures match the contract"
