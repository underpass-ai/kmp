#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FRAGMENT_NAME="${1:-}"

if [[ ! "${FRAGMENT_NAME}" =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
  echo "usage: $0 <lowercase-fragment-name>" >&2
  exit 2
fi

cd "${ROOT_DIR}"

COVERAGE_IGNORE_FILENAME_REGEX="${COVERAGE_IGNORE_FILENAME_REGEX:-kmp-conformance/.*|kmp-testkit/.*|kmp-tests-shared/.*|kmp-tests-kernel/.*|kmp-tests-paper/.*|kmp-transport-grpc/src/agentic_reference/.*}"
OUTPUT_DIR="target/llvm-cov/fragments"
OUTPUT_PATH="${OUTPUT_DIR}/${FRAGMENT_NAME}.info"

mkdir -p "${OUTPUT_DIR}"
cargo llvm-cov report \
  --locked \
  --ignore-filename-regex "${COVERAGE_IGNORE_FILENAME_REGEX}" \
  --lcov \
  --output-path "${OUTPUT_PATH}"

if [[ ! -s "${OUTPUT_PATH}" ]]; then
  echo "coverage fragment is empty: ${OUTPUT_PATH}" >&2
  exit 1
fi

echo "coverage fragment exported: ${OUTPUT_PATH}"
