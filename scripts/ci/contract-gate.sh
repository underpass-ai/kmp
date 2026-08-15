#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

resolve_breaking_ref() {
  if [[ -n "${CONTRACT_BREAKING_REF:-}" ]]; then
    printf '%s\n' "${CONTRACT_BREAKING_REF}"
    return 0
  fi

  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    git rev-parse origin/main
    return 0
  fi

  if git rev-parse --verify main >/dev/null 2>&1; then
    git rev-parse main
    return 0
  fi

  echo "could not resolve a baseline ref for buf breaking" >&2
  return 1
}

BREAKING_REF="$(resolve_breaking_ref)"

buf lint api
(
  cd api
  buf breaking . \
    --against "file://${ROOT_DIR}/.git#ref=${BREAKING_REF},subdir=api" \
    --limit-to-input-files \
    --path proto/underpass/rehydration/kernel/v1beta1
)
bash scripts/ci/check-vendored-contract.sh
bash scripts/ci/check-kernel-contract-policy.sh
cargo test -p kmp-proto --locked
