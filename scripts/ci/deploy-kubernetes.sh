#!/usr/bin/env bash

set -euo pipefail

CHART_PATH="${CHART_PATH:-charts/kmp}"
RELEASE_NAME="${RELEASE_NAME:?RELEASE_NAME is required}"
NAMESPACE="${NAMESPACE:?NAMESPACE is required}"
VALUES_FILE="${VALUES_FILE:-}"
IMAGE_TAG="${IMAGE_TAG:-}"
IMAGE_DIGEST="${IMAGE_DIGEST:-}"
HELM_TIMEOUT="${HELM_TIMEOUT:-10m}"
WAIT_FOR_ROLLOUT="${WAIT_FOR_ROLLOUT:-true}"
ATOMIC_DEPLOY="${ATOMIC_DEPLOY:-true}"
DRY_RUN="${DRY_RUN:-false}"

if [[ -n "${IMAGE_TAG}" && -n "${IMAGE_DIGEST}" ]]; then
  echo "set either IMAGE_TAG or IMAGE_DIGEST, not both" >&2
  exit 1
fi

if [[ -z "${IMAGE_TAG}" && -z "${IMAGE_DIGEST}" ]]; then
  echo "set IMAGE_TAG or IMAGE_DIGEST" >&2
  exit 1
fi

if [[ ! -d "${CHART_PATH}" ]]; then
  echo "chart path not found: ${CHART_PATH}" >&2
  exit 1
fi

if [[ -n "${VALUES_FILE}" && ! -f "${VALUES_FILE}" ]]; then
  echo "values file not found: ${VALUES_FILE}" >&2
  exit 1
fi

HELM_ARGS=(
  upgrade
  --install
  "${RELEASE_NAME}"
  "${CHART_PATH}"
  --namespace
  "${NAMESPACE}"
  --create-namespace
)

if [[ -n "${VALUES_FILE}" ]]; then
  HELM_ARGS+=(-f "${VALUES_FILE}")
fi

if [[ "${WAIT_FOR_ROLLOUT}" == "true" ]]; then
  HELM_ARGS+=(--wait --timeout "${HELM_TIMEOUT}")
fi

if [[ "${ATOMIC_DEPLOY}" == "true" && "${DRY_RUN}" != "true" ]]; then
  HELM_ARGS+=(--atomic)
fi

if [[ "${DRY_RUN}" == "true" ]]; then
  HELM_ARGS+=(--dry-run=server)
fi

if [[ -n "${IMAGE_DIGEST}" ]]; then
  HELM_ARGS+=(--set "image.digest=${IMAGE_DIGEST}" --set "image.tag=")
else
  HELM_ARGS+=(--set "image.tag=${IMAGE_TAG}" --set "image.digest=")
fi

helm "${HELM_ARGS[@]}"
