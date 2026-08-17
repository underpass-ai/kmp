#!/usr/bin/env bash
set -euo pipefail

readonly MAX_ATTEMPTS=3
readonly RETRY_DELAY_SECONDS=5

toolchain="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)"
if [[ -z "${toolchain}" ]]; then
  echo "rust-toolchain.toml does not declare a channel" >&2
  exit 1
fi

install_args=(
  toolchain install "${toolchain}"
  --profile minimal
  --no-self-update
)

if [[ -n "${KMP_RUST_COMPONENTS:-}" ]]; then
  IFS=', ' read -r -a requested_components <<< "${KMP_RUST_COMPONENTS}"
  for component in "${requested_components[@]}"; do
    [[ -z "${component}" ]] && continue
    install_args+=(--component "${component}")
  done
fi

if [[ -n "${KMP_RUST_TARGETS:-}" ]]; then
  IFS=', ' read -r -a requested_targets <<< "${KMP_RUST_TARGETS}"
  for target in "${requested_targets[@]}"; do
    [[ -z "${target}" ]] && continue
    install_args+=(--target "${target}")
  done
fi

for ((attempt = 1; attempt <= MAX_ATTEMPTS; attempt++)); do
  if rustup "${install_args[@]}"; then
    rustup default "${toolchain}"
    rustc --version --verbose
    exit 0
  fi

  if [[ "${attempt}" -eq "${MAX_ATTEMPTS}" ]]; then
    echo "Rust toolchain installation failed after ${MAX_ATTEMPTS} attempts" >&2
    exit 1
  fi

  delay=$((attempt * RETRY_DELAY_SECONDS))
  echo "Rust toolchain installation attempt ${attempt} failed; retrying in ${delay}s" >&2
  sleep "${delay}"
done
