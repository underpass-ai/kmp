# Kubernetes Transport Smoke

## Purpose

Provide a repeatable smoke workflow for the transport-security rollout on a real
cluster network, without port-forwarding.

The smoke path covers:

- inbound gRPC TLS
- inbound gRPC mTLS
- outbound NATS TLS startup
- outbound Valkey TLS write path

The script runs the probes from inside the cluster using a temporary `grpcurl`
pod.

## Script

- [`scripts/ci/kubernetes-transport-smoke.sh`](../../scripts/ci/kubernetes-transport-smoke.sh)

## Requirements

- `kubectl`
- `helm`
- `openssl`
- cluster access from the local machine
- a pullable kernel image tag or digest

For the outbound slice:

- a TLS-enabled NATS endpoint if `NATS_TLS_MODE` is not `disabled`
- a TLS-enabled Valkey endpoint if `VALKEY_TLS_ENABLED=true`
- any required client-cert secrets already present in the namespace

## Modes

- `grpc-server`
- `grpc-mutual`
- `outbound`
- `all`

`grpc-server`:

- deploys the chart with `tls.mode=server`
- generates a temporary CA and server certificate
- creates a temporary secret for the chart
- runs `grpcurl` from inside the cluster with the generated CA

`grpc-mutual`:

- deploys the chart with `tls.mode=mutual`
- first proves an anonymous client fails
- then proves a client with the generated certificate succeeds

`outbound`:

- deploys the chart with the selected inbound gRPC mode
- applies `natsTls.*` and `valkeyTls.*` Helm values
- verifies rollout succeeds
- calls `RehydrateSession` with `persistSnapshot=true`
- asserts `"snapshotPersisted": true` in the gRPC response

That snapshot write forces the kernel through a real Valkey path, so the smoke
does more than just prove that the pod booted.

For NATS, rollout success is the smoke signal: if the runtime cannot connect to
NATS with the configured TLS settings, the kernel does not finish startup.

## Basic Usage

gRPC TLS:

```bash
IMAGE_TAG=main \
bash scripts/ci/kubernetes-transport-smoke.sh grpc-server
```

gRPC mTLS:

```bash
IMAGE_TAG=main \
bash scripts/ci/kubernetes-transport-smoke.sh grpc-mutual
```

Outbound TLS with server-side gRPC TLS:

```bash
IMAGE_TAG=main \
NATS_TLS_MODE=server \
NATS_TLS_SECRET_NAME=kmp-nats-tls \
VALKEY_TLS_ENABLED=true \
VALKEY_TLS_SECRET_NAME=kmp-valkey-tls \
bash scripts/ci/kubernetes-transport-smoke.sh outbound
```

Outbound TLS with inbound gRPC mTLS:

```bash
IMAGE_TAG=main \
GRPC_SMOKE_MODE=mutual \
NATS_TLS_MODE=mutual \
NATS_TLS_SECRET_NAME=kmp-nats-tls \
VALKEY_TLS_ENABLED=true \
VALKEY_TLS_SECRET_NAME=kmp-valkey-tls \
bash scripts/ci/kubernetes-transport-smoke.sh outbound
```

Full sequence:

```bash
IMAGE_TAG=main \
NATS_TLS_MODE=server \
NATS_TLS_SECRET_NAME=kmp-nats-tls \
VALKEY_TLS_ENABLED=true \
VALKEY_TLS_SECRET_NAME=kmp-valkey-tls \
bash scripts/ci/kubernetes-transport-smoke.sh all
```

Full mTLS with OTel Collector and Loki:

```bash
IMAGE_TAG=mtls \
GRPC_SMOKE_MODE=mutual \
NATS_TLS_MODE=mutual \
NATS_TLS_SECRET_NAME=kmp-nats-tls \
VALKEY_TLS_ENABLED=true \
VALKEY_TLS_SECRET_NAME=kmp-valkey-tls \
OTEL_TLS_ENABLED=true \
OTEL_TLS_SECRET_NAME=kmp-otel-tls \
bash scripts/ci/kubernetes-transport-smoke.sh outbound
```

Use `VALUES_FILE` to provide a base values file without ingress conflicts:

```yaml
# /tmp/smoke-values.yaml
image:
  pullPolicy: Always
development:
  allowMutableImageTags: true
  allowInlineConnections: true
ingress:
  enabled: false
neo4j:
  enabled: true
  auth: "neo4j/underpassai"
nats:
  enabled: true
valkey:
  enabled: true
```

```bash
VALUES_FILE=/tmp/smoke-values.yaml \
CLEANUP_RELEASE=true \
IMAGE_TAG=mtls \
GRPC_SMOKE_MODE=mutual \
NATS_TLS_MODE=mutual \
NATS_TLS_SECRET_NAME=kmp-nats-tls \
VALKEY_TLS_ENABLED=true \
VALKEY_TLS_SECRET_NAME=kmp-valkey-tls \
OTEL_TLS_ENABLED=true \
OTEL_TLS_SECRET_NAME=kmp-otel-tls \
bash scripts/ci/kubernetes-transport-smoke.sh outbound
```

This verifies:
- gRPC mTLS (anonymous client rejected, authenticated client accepted)
- NATS mTLS (kernel connects with client cert)
- Valkey mTLS (snapshot write via `rediss://`)
- OTel Collector mTLS (receiver + Loki exporter)
- Kernel → Collector mTLS (OTLP env vars)

## Variables

General:

- `NAMESPACE` (default: `underpass-runtime`)
- `RELEASE_PREFIX` (default: `kmp-smoke`)
- `VALUES_FILE` (default: `charts/kmp/values.underpass-runtime.yaml`)
- `IMAGE_TAG`
- `IMAGE_DIGEST`
- `HELM_TIMEOUT` (default: `10m`)
- `GRPC_PORT` (default: `50054`)
- `PROBE_IMAGE` (default: `docker.io/fullstorydev/grpcurl:v1.9.3`)
- `IMAGE_PULL_SECRET` — optional; added to `imagePullSecrets` in the Helm override
- `CLEANUP_RELEASE` (default: `false`) — uninstall Helm release after smoke

Inbound gRPC:

- `GRPC_SMOKE_MODE=server|mutual`

Outbound NATS:

- `NATS_TLS_MODE=disabled|server|mutual`
- `NATS_TLS_SECRET_NAME`
- `NATS_TLS_FIRST=true|false`
- `NATS_TLS_CA_KEY`
- `NATS_TLS_CERT_KEY`
- `NATS_TLS_KEY_KEY`

Outbound Valkey:

- `VALKEY_TLS_ENABLED=true|false`
- `VALKEY_TLS_SECRET_NAME`
- `VALKEY_TLS_CA_KEY`
- `VALKEY_TLS_CERT_KEY`
- `VALKEY_TLS_KEY_KEY`

OTel Collector mTLS:

- `OTEL_TLS_ENABLED=true|false`
- `OTEL_TLS_SECRET_NAME`
- `OTEL_TLS_CA_KEY`
- `OTEL_TLS_CERT_KEY`
- `OTEL_TLS_KEY_KEY`

## Notes

- The script creates temporary gRPC certs locally with `openssl`.
- It creates temporary Kubernetes resources per smoke release:
  - `${release}-grpc-tls`
  - `${release}-grpc-proto`
- If `CLEANUP_RELEASE=true`, the Helm release is uninstalled after the smoke.
- If your deployment uses `secrets.existingSecret` for Valkey URIs, those
  secret values must already contain the final `rediss://` or `valkeys://`
  connection strings with matching `tls_*_path` query parameters.
