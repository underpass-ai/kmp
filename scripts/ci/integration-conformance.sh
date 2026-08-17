#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

cargo test \
  -p kmp-tests-kernel \
  --features container-tests \
  --test conformance_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1

# Relation-only projection is part of the kernel's async conformance boundary,
# but its end-to-end target used to exist outside every required CI job. Keep
# it beside the infrastructure conformance suite so a regression cannot merge
# while the documented test remains green only on a developer machine.
cargo test \
  -p kmp-tests-kernel \
  --features container-tests \
  --test relation_materialization_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1
