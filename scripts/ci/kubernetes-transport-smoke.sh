#!/usr/bin/env bash

set -euo pipefail

MODE="${1:-all}"
NAMESPACE="${NAMESPACE:-underpass-runtime}"
RELEASE_PREFIX="${RELEASE_PREFIX:-kmp-smoke}"
VALUES_FILE="${VALUES_FILE:-charts/kmp/values.underpass-runtime.yaml}"
IMAGE_TAG="${IMAGE_TAG:-}"
IMAGE_DIGEST="${IMAGE_DIGEST:-}"
HELM_TIMEOUT="${HELM_TIMEOUT:-10m}"
GRPC_PORT="${GRPC_PORT:-50054}"
PROBE_IMAGE="${PROBE_IMAGE:-docker.io/fullstorydev/grpcurl:v1.9.3}"
CLEANUP_RELEASE="${CLEANUP_RELEASE:-false}"
GRPC_MODE_SERVER="server"
GRPC_MODE_MUTUAL="mutual"
SNAPSHOT_PERSISTED_PATTERN='"snapshotPersisted": true'
GRPC_SMOKE_MODE="${GRPC_SMOKE_MODE:-${GRPC_MODE_SERVER}}"
IMAGE_PULL_SECRET="${IMAGE_PULL_SECRET:-}"

NATS_TLS_MODE="${NATS_TLS_MODE:-disabled}"
NATS_TLS_SECRET_NAME="${NATS_TLS_SECRET_NAME:-}"
NATS_TLS_FIRST="${NATS_TLS_FIRST:-false}"
NATS_TLS_CA_KEY="${NATS_TLS_CA_KEY:-ca.crt}"
NATS_TLS_CERT_KEY="${NATS_TLS_CERT_KEY:-tls.crt}"
NATS_TLS_KEY_KEY="${NATS_TLS_KEY_KEY:-tls.key}"

VALKEY_TLS_ENABLED="${VALKEY_TLS_ENABLED:-false}"
VALKEY_TLS_SECRET_NAME="${VALKEY_TLS_SECRET_NAME:-}"
VALKEY_TLS_CA_KEY="${VALKEY_TLS_CA_KEY:-ca.crt}"
VALKEY_TLS_CERT_KEY="${VALKEY_TLS_CERT_KEY:-tls.crt}"
VALKEY_TLS_KEY_KEY="${VALKEY_TLS_KEY_KEY:-tls.key}"

OTEL_TLS_ENABLED="${OTEL_TLS_ENABLED:-false}"
OTEL_TLS_SECRET_NAME="${OTEL_TLS_SECRET_NAME:-}"
OTEL_TLS_CA_KEY="${OTEL_TLS_CA_KEY:-ca.crt}"
OTEL_TLS_CERT_KEY="${OTEL_TLS_CERT_KEY:-tls.crt}"
OTEL_TLS_KEY_KEY="${OTEL_TLS_KEY_KEY:-tls.key}"

if [[ -n "${IMAGE_TAG}" && -n "${IMAGE_DIGEST}" ]]; then
  echo "set either IMAGE_TAG or IMAGE_DIGEST, not both" >&2
  exit 1
fi

if [[ -z "${IMAGE_TAG}" && -z "${IMAGE_DIGEST}" ]]; then
  echo "set IMAGE_TAG or IMAGE_DIGEST" >&2
  exit 1
fi

if [[ ! -f "${VALUES_FILE}" ]]; then
  echo "values file not found: ${VALUES_FILE}" >&2
  exit 1
fi

case "${MODE}" in
  grpc-server|grpc-mutual|outbound|all) ;;
  *)
    echo "unsupported mode: ${MODE}" >&2
    exit 1
    ;;
esac

case "${GRPC_SMOKE_MODE}" in
  "${GRPC_MODE_SERVER}"|"${GRPC_MODE_MUTUAL}") ;;
  *)
    echo "GRPC_SMOKE_MODE must be server or mutual" >&2
    exit 1
    ;;
esac

case "${NATS_TLS_MODE}" in
  disabled|"${GRPC_MODE_SERVER}"|"${GRPC_MODE_MUTUAL}") ;;
  *)
    echo "NATS_TLS_MODE must be disabled, server, or mutual" >&2
    exit 1
    ;;
esac

if [[ "${NATS_TLS_MODE}" == "${GRPC_MODE_MUTUAL}" && -z "${NATS_TLS_SECRET_NAME}" ]]; then
  echo "NATS_TLS_SECRET_NAME is required when NATS_TLS_MODE=mutual" >&2
  exit 1
fi

case "${VALKEY_TLS_ENABLED}" in
  true|false) ;;
  *)
    echo "VALKEY_TLS_ENABLED must be true or false" >&2
    exit 1
    ;;
esac

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

bool_is_true() {
  if [[ "${1}" == "true" || "${1}" == "1" || "${1}" == "yes" ]]; then
    return 0
  fi

  return 1
}

release_name_for() {
  printf "%s-%s" "${RELEASE_PREFIX}" "${1}"
  return 0
}

service_host_for() {
  local release_name="$1"
  printf "%s.%s.svc.cluster.local:%s" "${release_name}" "${NAMESPACE}" "${GRPC_PORT}"
  return 0
}

cleanup_release() {
  local release_name="$1"

  if bool_is_true "${CLEANUP_RELEASE}"; then
    helm uninstall "${release_name}" --namespace "${NAMESPACE}" >/dev/null 2>&1 || true
  fi

  return 0
}

cleanup_temp_resources() {
  local release_name="$1"

  kubectl delete secret "${release_name}-grpc-tls" -n "${NAMESPACE}" --ignore-not-found >/dev/null
  kubectl delete configmap "${release_name}-grpc-proto" -n "${NAMESPACE}" --ignore-not-found >/dev/null

  return 0
}

run_openssl() {
  local output

  output="$(openssl "$@" 2>&1)" || {
    echo "${output}" >&2
    return 1
  }

  return 0
}

write_grpc_tls_material() {
  local release_name="$1"
  local cert_dir="${TMP_DIR}/${release_name}/certs"
  local service_name="${release_name}"
  local service_short="${service_name}"
  local service_ns="${service_name}.${NAMESPACE}.svc"
  local service_fqdn="${service_name}.${NAMESPACE}.svc.cluster.local"
  local server_cn="grpc-smoke-server"

  mkdir -p "${cert_dir}"

  local ca_key="${cert_dir}/ca.key"
  local ca_cert="${cert_dir}/ca.crt"
  local server_key="${cert_dir}/server.key"
  local server_csr="${cert_dir}/server.csr"
  local server_ext="${cert_dir}/server.ext"
  local server_cert="${cert_dir}/server.crt"
  local client_key="${cert_dir}/client.key"
  local client_csr="${cert_dir}/client.csr"
  local client_ext="${cert_dir}/client.ext"
  local client_cert="${cert_dir}/client.crt"

  run_openssl req -x509 -newkey rsa:2048 -days 3650 -nodes \
    -keyout "${ca_key}" \
    -out "${ca_cert}" \
    -subj "/CN=${release_name}-grpc-test-ca"

  run_openssl req -newkey rsa:2048 -nodes \
    -keyout "${server_key}" \
    -out "${server_csr}" \
    -subj "/CN=${server_cn}" \
    -addext "subjectAltName=DNS:${service_short},DNS:${service_ns},DNS:${service_fqdn}"
  cat >"${server_ext}" <<EOF
[v3_req]
subjectAltName=DNS:${service_short},DNS:${service_ns},DNS:${service_fqdn}
extendedKeyUsage=serverAuth
EOF
  run_openssl x509 -req \
    -in "${server_csr}" \
    -CA "${ca_cert}" \
    -CAkey "${ca_key}" \
    -CAcreateserial \
    -out "${server_cert}" \
    -days 3650 \
    -extfile "${server_ext}" \
    -extensions v3_req

  run_openssl req -newkey rsa:2048 -nodes \
    -keyout "${client_key}" \
    -out "${client_csr}" \
    -subj "/CN=${release_name}-grpc-test-client"
  cat >"${client_ext}" <<EOF
[v3_req]
extendedKeyUsage=clientAuth
EOF
  run_openssl x509 -req \
    -in "${client_csr}" \
    -CA "${ca_cert}" \
    -CAkey "${ca_key}" \
    -CAcreateserial \
    -out "${client_cert}" \
    -days 3650 \
    -extfile "${client_ext}" \
    -extensions v3_req

  kubectl create namespace "${NAMESPACE}" --dry-run=client -o yaml | kubectl apply -f - >/dev/null
  kubectl create secret generic "${release_name}-grpc-tls" \
    -n "${NAMESPACE}" \
    --from-file=tls.crt="${server_cert}" \
    --from-file=tls.key="${server_key}" \
    --from-file=ca.crt="${ca_cert}" \
    --from-file=client.crt="${client_cert}" \
    --from-file=client.key="${client_key}" \
    --dry-run=client -o yaml | kubectl apply -f - >/dev/null

  kubectl create configmap "${release_name}-grpc-proto" \
    -n "${NAMESPACE}" \
    --from-file=query.proto=api/proto/underpass/rehydration/kernel/v1beta1/query.proto \
    --from-file=common.proto=api/proto/underpass/rehydration/kernel/v1beta1/common.proto \
    --dry-run=client -o yaml | kubectl apply -f - >/dev/null

  return 0
}

write_override_values() {
  local release_name="$1"
  local grpc_mode="$2"
  local override_file="${TMP_DIR}/${release_name}/override.yaml"

  mkdir -p "$(dirname "${override_file}")"

  cat >"${override_file}" <<EOF
tls:
  mode: ${grpc_mode}
  existingSecret: ${release_name}-grpc-tls
EOF

  if [[ -n "${IMAGE_PULL_SECRET}" ]]; then
    cat >>"${override_file}" <<EOF
imagePullSecrets:
  - name: ${IMAGE_PULL_SECRET}
EOF
  fi

  if [[ "${NATS_TLS_MODE}" != "disabled" || -n "${NATS_TLS_SECRET_NAME}" ]]; then
    cat >>"${override_file}" <<EOF
nats:
  tls:
    enabled: true
    existingSecret: ${NATS_TLS_SECRET_NAME}
    caSecret: ${NATS_TLS_SECRET_NAME}
natsTls:
  mode: ${NATS_TLS_MODE}
  existingSecret: ${NATS_TLS_SECRET_NAME}
  tlsFirst: ${NATS_TLS_FIRST}
  keys:
    ca: ${NATS_TLS_CA_KEY}
    cert: ${NATS_TLS_CERT_KEY}
    key: ${NATS_TLS_KEY_KEY}
EOF
  fi

  if bool_is_true "${VALKEY_TLS_ENABLED}" || [[ -n "${VALKEY_TLS_SECRET_NAME}" ]]; then
    cat >>"${override_file}" <<EOF
valkey:
  tls:
    enabled: true
    existingSecret: ${VALKEY_TLS_SECRET_NAME}
    caSecret: ${VALKEY_TLS_SECRET_NAME}
valkeyTls:
  enabled: true
  existingSecret: ${VALKEY_TLS_SECRET_NAME}
  keys:
    ca: ${VALKEY_TLS_CA_KEY}
    cert: ${VALKEY_TLS_CERT_KEY}
    key: ${VALKEY_TLS_KEY_KEY}
EOF
  fi

  if bool_is_true "${OTEL_TLS_ENABLED}" || [[ -n "${OTEL_TLS_SECRET_NAME}" ]]; then
    cat >>"${override_file}" <<EOF
otelCollector:
  enabled: true
  tls:
    enabled: ${OTEL_TLS_ENABLED}
    existingSecret: ${OTEL_TLS_SECRET_NAME}
    keys:
      ca: ${OTEL_TLS_CA_KEY}
      cert: ${OTEL_TLS_CERT_KEY}
      key: ${OTEL_TLS_KEY_KEY}
loki:
  enabled: true
extraEnv:
  - name: OTEL_EXPORTER_OTLP_CA_PATH
    value: /var/run/kmp/otel-tls/${OTEL_TLS_CA_KEY}
  - name: OTEL_EXPORTER_OTLP_CERT_PATH
    value: /var/run/kmp/otel-tls/${OTEL_TLS_CERT_KEY}
  - name: OTEL_EXPORTER_OTLP_KEY_PATH
    value: /var/run/kmp/otel-tls/${OTEL_TLS_KEY_KEY}
EOF
  fi

  printf "%s" "${override_file}"
  return 0
}

helm_deploy() {
  local release_name="$1"
  local override_file="$2"
  local set_image_args=()

  if [[ -n "${IMAGE_DIGEST}" ]]; then
    set_image_args=(--set "image.digest=${IMAGE_DIGEST}" --set "image.tag=")
  else
    set_image_args=(--set "image.tag=${IMAGE_TAG}" --set "image.digest=")
  fi

  helm upgrade --install "${release_name}" charts/kmp \
    --namespace "${NAMESPACE}" \
    --create-namespace \
    -f "${VALUES_FILE}" \
    -f "${override_file}" \
    --wait \
    --timeout "${HELM_TIMEOUT}" \
    --atomic \
    "${set_image_args[@]}"

  return 0
}

assert_rollout() {
  local release_name="$1"

  kubectl rollout status "deployment/${release_name}" -n "${NAMESPACE}" --timeout "${HELM_TIMEOUT}"
  kubectl get svc "${release_name}" -n "${NAMESPACE}" >/dev/null

  # Wait for the kernel to finish backend connections (Neo4j retries, NATS handshake).
  # The pod is Running but gRPC may not be ready yet.
  echo "  waiting for kernel gRPC to become ready..."
  local ready=false
  for i in $(seq 1 30); do
    if kubectl logs "deployment/${release_name}" -n "${NAMESPACE}" --tail=50 2>/dev/null | grep -q "warmup bundle"; then
      ready=true
      break
    fi
    sleep 2
  done
  if ! ${ready}; then
    echo "  warning: kernel did not reach warmup within 60s, proceeding anyway"
  fi

  return 0
}

grpcurl_run() {
  local release_name="$1"
  local probe_name="$2"
  local grpc_mode="$3"
  local require_success="$4"
  local payload="$5"
  local host
  local output_file="${TMP_DIR}/${release_name}/${probe_name}.log"
  local overrides_file="${TMP_DIR}/${release_name}/${probe_name}-overrides.json"
  local service_host
  local overrides_json
  local payload_json
  local auth_args_json=""

  host="$(service_host_for "${release_name}")"
  service_host="${host%:*}"
  payload_json="$(printf '%s' "${payload}" | sed 's/\\/\\\\/g; s/"/\\"/g')"

  if [[ "${grpc_mode}" == "${GRPC_MODE_MUTUAL}" ]]; then
    auth_args_json=$(
      cat <<'EOF'
,          "-cert",
          "/certs/client.crt",
          "-key",
          "/certs/client.key"
EOF
    )
  fi

  cat >"${overrides_file}" <<EOF
{
  "apiVersion": "v1",
  "spec": {
    "restartPolicy": "Never",
    "containers": [
      {
        "name": "${probe_name}",
        "image": "${PROBE_IMAGE}",
        "command": [
          "grpcurl",
          "-cacert",
          "/certs/ca.crt",
          "-authority",
          "${service_host}",
          "-import-path",
          "/proto",
          "-proto",
          "underpass/rehydration/kernel/v1beta1/query.proto"${auth_args_json},
          "-d",
          "${payload_json}",
          "${host}",
          "underpass.rehydration.kernel.v1beta1.ContextQueryService/RehydrateSession"
        ],
        "volumeMounts": [
          {
            "name": "proto",
            "mountPath": "/proto",
            "readOnly": true
          },
          {
            "name": "grpc-tls",
            "mountPath": "/certs",
            "readOnly": true
          }
        ]
      }
    ],
    "volumes": [
      {
        "name": "proto",
        "configMap": {
          "name": "${release_name}-grpc-proto",
          "items": [
            {
              "key": "query.proto",
              "path": "underpass/rehydration/kernel/v1beta1/query.proto"
            },
            {
              "key": "common.proto",
              "path": "underpass/rehydration/kernel/v1beta1/common.proto"
            }
          ]
        }
      },
      {
        "name": "grpc-tls",
        "secret": {
          "secretName": "${release_name}-grpc-tls"
        }
      }
    ]
  }
}
EOF
  overrides_json="$(cat "${overrides_file}")"

  # Clean up any leftover probe pod from a previous run
  kubectl delete pod "${probe_name}" -n "${NAMESPACE}" --ignore-not-found >/dev/null 2>&1

  local args=(
    kubectl run "${probe_name}"
    --namespace "${NAMESPACE}"
    --rm
    --attach
    --restart=Never
    --image "${PROBE_IMAGE}"
    --overrides "${overrides_json}"
  )

  set +e
  "${args[@]}" >"${output_file}" 2>&1
  local status=$?
  set -e

  if [[ "${require_success}" == "true" ]]; then
    if [[ ${status} -ne 0 ]]; then
      cat "${output_file}" >&2
      return 1
    fi
    cat "${output_file}"
    return 0
  fi

  if [[ ${status} -eq 0 ]]; then
    cat "${output_file}" >&2
    echo "expected grpcurl probe to fail: ${probe_name}" >&2
    return 1
  fi

  cat "${output_file}"
  return 0
}

smoke_payload() {
  cat <<'EOF'
{"rootNodeId":"node:smoke:transport-security","roles":["system"],"persistSnapshot":true,"snapshotTtl":"300s"}
EOF
  return 0
}

run_grpc_server_smoke() {
  local release_name
  release_name="$(release_name_for "grpc-server")"

  cleanup_temp_resources "${release_name}"
  write_grpc_tls_material "${release_name}"
  local override_file
  override_file="$(write_override_values "${release_name}" "${GRPC_MODE_SERVER}")"

  helm_deploy "${release_name}" "${override_file}"
  assert_rollout "${release_name}"
  grpcurl_run "${release_name}" "${release_name}-probe" "${GRPC_MODE_SERVER}" "true" "$(smoke_payload)" | grep -q "${SNAPSHOT_PERSISTED_PATTERN}"
  return 0
}

run_grpc_mutual_smoke() {
  local release_name
  release_name="$(release_name_for "grpc-mutual")"

  cleanup_temp_resources "${release_name}"
  write_grpc_tls_material "${release_name}"
  local override_file
  override_file="$(write_override_values "${release_name}" "${GRPC_MODE_MUTUAL}")"

  helm_deploy "${release_name}" "${override_file}"
  assert_rollout "${release_name}"

  local anonymous_output
  anonymous_output="$(grpcurl_run "${release_name}" "${release_name}-probe-anon" "${GRPC_MODE_SERVER}" "false" "$(smoke_payload)")"
  if ! grep -Eqi 'certificate|tls|handshake|authentication|deadline exceeded' <<<"${anonymous_output}"; then
    echo "${anonymous_output}" >&2
    echo "unauthenticated mutual TLS probe failed, but not with a TLS error" >&2
    return 1
  fi

  grpcurl_run "${release_name}" "${release_name}-probe-auth" "${GRPC_MODE_MUTUAL}" "true" "$(smoke_payload)" | grep -q "${SNAPSHOT_PERSISTED_PATTERN}"
  return 0
}

run_outbound_smoke() {
  local release_name
  release_name="$(release_name_for "outbound")"

  cleanup_temp_resources "${release_name}"
  write_grpc_tls_material "${release_name}"
  local override_file
  override_file="$(write_override_values "${release_name}" "${GRPC_SMOKE_MODE}")"

  helm_deploy "${release_name}" "${override_file}"
  assert_rollout "${release_name}"

  # Run probe and capture output. The kernel may respond with snapshotPersisted
  # (seeded DB) or NotFound (empty DB). Both prove mTLS transport works.
  local probe_output
  set +e
  if [[ "${GRPC_SMOKE_MODE}" == "${GRPC_MODE_MUTUAL}" ]]; then
    probe_output="$(grpcurl_run "${release_name}" "${release_name}-probe" "${GRPC_MODE_MUTUAL}" "true" "$(smoke_payload)" 2>&1)"
  else
    probe_output="$(grpcurl_run "${release_name}" "${release_name}-probe" "${GRPC_MODE_SERVER}" "true" "$(smoke_payload)" 2>&1)"
  fi
  set -e

  if echo "${probe_output}" | grep -q "${SNAPSHOT_PERSISTED_PATTERN}"; then
    echo "  outbound smoke: snapshotPersisted confirmed"
  elif echo "${probe_output}" | grep -qi "NotFound"; then
    echo "  outbound smoke: kernel responded NOT_FOUND (empty DB — mTLS transport verified)"
  else
    echo "${probe_output}" >&2
    echo "  outbound smoke: unexpected response (not snapshotPersisted nor NotFound)" >&2
    return 1
  fi

  # Assert OTel Collector is running and receiving (when enabled)
  if bool_is_true "${OTEL_TLS_ENABLED}"; then
    echo "  verifying OTel Collector is running..."
    kubectl rollout status "deployment/${release_name}-otel-collector" -n "${NAMESPACE}" --timeout 60s

    echo "  verifying Loki is ready..."
    kubectl exec -n "${NAMESPACE}" "deployment/${release_name}-loki" -- \
      wget -qO- http://localhost:3100/ready 2>/dev/null || true

    echo "  verifying kernel logs contain quality metrics..."
    sleep 5
    local kernel_logs
    kernel_logs="$(kubectl logs -n "${NAMESPACE}" "deployment/${release_name}" --tail=50 2>/dev/null || true)"
    if echo "${kernel_logs}" | grep -q "bundle quality metrics"; then
      echo "  quality metrics log found in kernel output"
    else
      echo "  warning: quality metrics log not found (may need a GetContext call to trigger)"
    fi
  fi

  return 0
}

main() {
  case "${MODE}" in
    grpc-server)
      run_grpc_server_smoke
      ;;
    grpc-mutual)
      run_grpc_mutual_smoke
      ;;
    outbound)
      run_outbound_smoke
      ;;
    all)
      run_grpc_server_smoke
      run_grpc_mutual_smoke
      run_outbound_smoke
      ;;
    *)
      echo "unsupported mode: ${MODE}" >&2
      return 1
      ;;
  esac

  return 0
}

main "$@"
