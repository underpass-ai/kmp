#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python="${PYTHON:-python3}"

if [[ "$#" -ne 2 ]]; then
  printf '%s\n' \
    "usage: final-regeneration-gate.sh --scratch REPOSITORY_TMP_DIRECTORY" >&2
  exit 2
fi
if [[ "$1" != "--scratch" || -z "$2" ]]; then
  printf '%s\n' \
    "usage: final-regeneration-gate.sh --scratch REPOSITORY_TMP_DIRECTORY" >&2
  exit 2
fi

exec "${python}" "${script_dir}/final_regeneration_gate.py" --scratch "$2"
