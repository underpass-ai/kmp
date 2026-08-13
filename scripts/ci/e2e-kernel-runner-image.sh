#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_TAG="${1:-kmp-e2e-runner:ci}"
: "${CONTAINER_RUNTIME:=auto}"

select_container_cli() {
  case "${CONTAINER_RUNTIME}" in
    auto)
      if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
        echo "docker"
        return 0
      fi
      if command -v podman >/dev/null 2>&1; then
        echo "podman"
        return 0
      fi
      echo "no supported container runtime found; install docker or podman" >&2
      return 1
      ;;
    docker)
      command -v docker >/dev/null 2>&1 || {
        echo "docker is not installed" >&2
        return 1
      }
      docker info >/dev/null 2>&1 || {
        echo "docker is installed but not available" >&2
        return 1
      }
      echo "docker"
      ;;
    podman)
      command -v podman >/dev/null 2>&1 || {
        echo "podman is not installed" >&2
        return 1
      }
      echo "podman"
      ;;
    *)
      echo "unsupported CONTAINER_RUNTIME=${CONTAINER_RUNTIME}; expected auto, docker, or podman" >&2
      return 1
      ;;
  esac
}

CONTAINER_CLI="$(select_container_cli)"

if [[ "${CONTAINER_CLI}" == "podman" ]]; then
  export XDG_RUNTIME_DIR="${TMPDIR:-/tmp}/podman-runtime-${UID}"
  mkdir -p "${XDG_RUNTIME_DIR}"
fi

echo "building kernel contract runner image with ${CONTAINER_CLI}" >&2

"${CONTAINER_CLI}" build \
  --file e2e/kernel-runner/Dockerfile \
  --tag "${IMAGE_TAG}" \
  "${ROOT_DIR}"
