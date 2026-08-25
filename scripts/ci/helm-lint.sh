#!/usr/bin/env bash
set -euo pipefail

CHART_PATH="${1:-charts/kmp}"
DEV_VALUES="${CHART_PATH}/values.dev.yaml"
UNDERPASS_RUNTIME_VALUES="${CHART_PATH}/values.underpass-runtime.yaml"
UNDERPASS_RUNTIME_MTLS_VALUES="${CHART_PATH}/values.underpass-runtime.mtls.example.yaml"
UNDERPASS_RUNTIME_SECURE_VALUES="${CHART_PATH}/values.underpass-runtime.secure.example.yaml"
DEFAULT_ERR="${TMPDIR:-/tmp}/kmp-helm-default.err"

helm lint "${CHART_PATH}" -f "${DEV_VALUES}"
helm template kmp "${CHART_PATH}" -f "${DEV_VALUES}" >/tmp/kmp-helm-template.yaml

SERVER_TLS_VALUES="${TMPDIR:-/tmp}/kmp-helm-server-tls.yaml"
MUTUAL_TLS_VALUES="${TMPDIR:-/tmp}/kmp-helm-mutual-tls.yaml"
OUTBOUND_TLS_VALUES="${TMPDIR:-/tmp}/kmp-helm-outbound-tls.yaml"
INGRESS_VALUES="${TMPDIR:-/tmp}/kmp-helm-ingress.yaml"
NEO4J_TLS_VALUES="${TMPDIR:-/tmp}/kmp-helm-neo4j-tls.yaml"
SERVICE_ANNOTATIONS_VALUES="${TMPDIR:-/tmp}/kmp-helm-service-annotations.yaml"
MCP_HTTP_VALUES="${TMPDIR:-/tmp}/kmp-helm-mcp-http.yaml"
PINNED_IMAGE_VALUES="${TMPDIR:-/tmp}/kmp-helm-pinned-image.yaml"

cat >"${SERVER_TLS_VALUES}" <<'EOF'
image:
  tag: latest
tls:
  mode: server
  existingSecret: grpc-server-tls
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

cat >"${MUTUAL_TLS_VALUES}" <<'EOF'
image:
  tag: latest
tls:
  mode: mutual
  existingSecret: grpc-mutual-tls
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${SERVER_TLS_VALUES}" >/tmp/kmp-helm-server-tls-template.yaml
helm template kmp "${CHART_PATH}" -f "${MUTUAL_TLS_VALUES}" >/tmp/kmp-helm-mutual-tls-template.yaml

cat >"${OUTBOUND_TLS_VALUES}" <<'EOF'
image:
  tag: latest
natsTls:
  mode: mutual
  existingSecret: nats-client-tls
  tlsFirst: true
  keys:
    ca: ca.crt
    cert: tls.crt
    key: tls.key
valkeyTls:
  enabled: true
  existingSecret: valkey-client-tls
  keys:
    ca: ca.crt
    cert: tls.crt
    key: tls.key
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${OUTBOUND_TLS_VALUES}" >/tmp/kmp-helm-outbound-tls-template.yaml

cat >"${INGRESS_VALUES}" <<'EOF'
image:
  tag: latest
ingress:
  enabled: true
  className: nginx
  annotations:
    nginx.ingress.kubernetes.io/backend-protocol: GRPC
  hosts:
    - host: kmp.example.com
      paths:
        - path: /
          pathType: Prefix
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${INGRESS_VALUES}" >/tmp/kmp-helm-ingress-template.yaml

cat >"${NEO4J_TLS_VALUES}" <<'EOF'
image:
  tag: latest
neo4jTls:
  enabled: true
  existingSecret: neo4j-ca
  keys:
    ca: ca.crt
connections:
  graphUri: bolt+s://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${NEO4J_TLS_VALUES}" >/tmp/kmp-helm-neo4j-tls-template.yaml

cat >"${SERVICE_ANNOTATIONS_VALUES}" <<'EOF'
image:
  tag: latest
service:
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-scheme: internal
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${SERVICE_ANNOTATIONS_VALUES}" >/tmp/kmp-helm-service-annotations-template.yaml

cat >"${MCP_HTTP_VALUES}" <<'EOF'
image:
  tag: latest
tls:
  mode: mutual
  existingSecret: grpc-server-tls
mcpHttp:
  enabled: true
  publicUrl: https://kmp.example.com/mcp
  auth:
    issuer: https://identity.example.com/
    audience: https://kmp.example.com/mcp
    allowedOrigins:
      - https://client.example.com
  grpcTls:
    existingSecret: grpc-client-tls
    domainName: kmp
  ingress:
    enabled: true
    className: nginx
    annotations:
      nginx.ingress.kubernetes.io/backend-protocol: HTTP
    hosts:
      - host: kmp.example.com
        paths:
          - path: /mcp
            pathType: Exact
          - path: /.well-known/oauth-protected-resource
            pathType: Prefix
connections:
  graphUri: neo4j://neo4j:7687
  detailUri: redis://valkey:6379
  snapshotUri: redis://valkey:6379
  runtimeStateUri: redis://valkey:6379
  natsUrl: nats://nats:4222
development:
  allowMutableImageTags: true
  allowInlineConnections: true
EOF

helm template kmp "${CHART_PATH}" -f "${MCP_HTTP_VALUES}" >/tmp/kmp-helm-mcp-http-template.yaml

cat >"${PINNED_IMAGE_VALUES}" <<'EOF'
image:
  tag: latest
development:
  allowMutableImageTags: true
EOF

helm template kmp "${CHART_PATH}" -f "${UNDERPASS_RUNTIME_VALUES}" -f "${PINNED_IMAGE_VALUES}" >/tmp/kmp-helm-underpass-runtime-template.yaml
helm template kmp "${CHART_PATH}" -f "${UNDERPASS_RUNTIME_MTLS_VALUES}" -f "${PINNED_IMAGE_VALUES}" >/tmp/kmp-helm-underpass-runtime-mtls-template.yaml
helm template kmp "${CHART_PATH}" -f "${UNDERPASS_RUNTIME_SECURE_VALUES}" -f "${PINNED_IMAGE_VALUES}" >/tmp/kmp-helm-underpass-runtime-secure-template.yaml

grep -q "NATS_TLS_MODE" /tmp/kmp-helm-outbound-tls-template.yaml
grep -q "NATS_TLS_CERT_PATH" /tmp/kmp-helm-outbound-tls-template.yaml
grep -q "rediss://valkey:6379?tls_ca_path=/var/run/kmp/valkey-tls/ca.crt&tls_cert_path=/var/run/kmp/valkey-tls/tls.crt&tls_key_path=/var/run/kmp/valkey-tls/tls.key" /tmp/kmp-helm-outbound-tls-template.yaml
grep -q "name: nats-tls" /tmp/kmp-helm-outbound-tls-template.yaml
grep -q "name: valkey-tls" /tmp/kmp-helm-outbound-tls-template.yaml
grep -q "kind: Ingress" /tmp/kmp-helm-ingress-template.yaml
grep -q "nginx.ingress.kubernetes.io/backend-protocol: GRPC" /tmp/kmp-helm-ingress-template.yaml
grep -q "host: \"kmp.example.com\"" /tmp/kmp-helm-ingress-template.yaml
grep -q "bolt+s://neo4j:7687?tls_ca_path=/var/run/kmp/neo4j-tls/ca.crt" /tmp/kmp-helm-neo4j-tls-template.yaml
grep -q "name: neo4j-tls" /tmp/kmp-helm-neo4j-tls-template.yaml
grep -q "service.beta.kubernetes.io/aws-load-balancer-scheme: internal" /tmp/kmp-helm-service-annotations-template.yaml
grep -q "app.kubernetes.io/component: mcp-http" /tmp/kmp-helm-mcp-http-template.yaml
grep -q 'command:.*kmp-mcp-http' /tmp/kmp-helm-mcp-http-template.yaml
grep -q "name: KMP_MCP_HTTP_AUTH_ISSUER" /tmp/kmp-helm-mcp-http-template.yaml
grep -q "name: KMP_KERNEL_GRPC_TLS_CERT_PATH" /tmp/kmp-helm-mcp-http-template.yaml
grep -q "secretName: \"grpc-client-tls\"" /tmp/kmp-helm-mcp-http-template.yaml
grep -q "kind: NetworkPolicy" /tmp/kmp-helm-mcp-http-template.yaml
grep -q 'path: "/mcp"' /tmp/kmp-helm-mcp-http-template.yaml
grep -q "host: \"kmp.underpassai.com\"" /tmp/kmp-helm-underpass-runtime-template.yaml
grep -q "nginx.ingress.kubernetes.io/backend-protocol: GRPC" /tmp/kmp-helm-underpass-runtime-template.yaml
grep -q "OTEL_EXPORTER_OTLP_ENDPOINT" /tmp/kmp-helm-underpass-runtime-mtls-template.yaml
grep -q "value: \"https://kmp-otel:4317\"" /tmp/kmp-helm-underpass-runtime-mtls-template.yaml
grep -q "name: otel-tls" /tmp/kmp-helm-underpass-runtime-mtls-template.yaml
grep -q "mountPath: \"/var/run/kmp/otel-tls\"" /tmp/kmp-helm-underpass-runtime-mtls-template.yaml
grep -q "secretName: \"kmp-otel-tls\"" /tmp/kmp-helm-underpass-runtime-mtls-template.yaml
grep -q "neo4j+s://neo4j:underpassai@neo4j.swe-ai-fleet.svc.cluster.local:7687?tls_ca_path=/var/run/kmp/neo4j-tls/ca.crt" /tmp/kmp-helm-underpass-runtime-secure-template.yaml
grep -q "secretName: kmp-ingress-tls" /tmp/kmp-helm-underpass-runtime-secure-template.yaml
grep -q "secretName: \"kmp-neo4j-tls\"" /tmp/kmp-helm-underpass-runtime-secure-template.yaml

if helm template kmp "${CHART_PATH}" > /dev/null 2>"${DEFAULT_ERR}"; then
  echo "default chart render unexpectedly succeeded" >&2
  exit 1
fi

grep -q "set image.tag or image.digest" "${DEFAULT_ERR}"
