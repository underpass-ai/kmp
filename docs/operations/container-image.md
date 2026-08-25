# Container Image

## Image

Registry: `ghcr.io/underpass-ai/kmp`

Published tags:

- `latest` — default branch
- `main`
- `sha-<short-commit>`
- `v*` — version tags (e.g. `v0.1.0`)

## Contents

The image packages three binaries:

| Binary | Path | Purpose |
|:-------|:-----|:--------|
| `kmp-server` | `/usr/local/bin/kmp-server` | Kernel server (entrypoint) |
| `kmp-mcp-http` | `/usr/local/bin/kmp-mcp-http` | Authenticated Streamable HTTP MCP adapter (selected by the gateway Deployment) |
| `runtime-reference-client` | `/usr/local/bin/runtime-reference-client` | Reference agentic context client |

Base: `debian:bookworm-slim` with `ca-certificates` and `tini` (PID 1).
Runs as non-root user `rehydration` (uid created at build time).

## Port

- gRPC: `50054`
- HTTP MCP: `8080` when `kmp-mcp-http` is selected

## Environment Variables

### Defaults baked into the image (Dockerfile ENV)

These have defaults in the image. Override via `-e` or Helm values:

| Variable | Default | Description |
|:---------|:--------|:------------|
| `KMP_SERVICE_NAME` | `kmp` | Service name in logs and OTel |
| `KMP_GRPC_BIND` | `0.0.0.0:50054` | gRPC listen address |
| `KMP_GRAPH_URI` | `neo4j://neo4j:7687` | Neo4j connection URI |
| `KMP_DETAIL_URI` | `redis://valkey:6379` | Valkey for node detail |
| `KMP_SNAPSHOT_URI` | `redis://valkey:6379` | Valkey for snapshots |
| `KMP_RUNTIME_STATE_URI` | `redis://valkey:6379` | Valkey for projection state |
| `KMP_EVENTS_PREFIX` | `rehydration` | NATS subject prefix |
| `NATS_URL` | `nats://nats:4222` | NATS JetStream connection |

### Runtime-only (no default in image)

These are read by the server at startup but have no `ENV` in the Dockerfile:

| Variable | Default (code) | Description |
|:---------|:---------------|:------------|
| `KMP_LOG_FORMAT` | `compact` | `json` (for Loki), `pretty`, `compact` |
| `KMP_GRPC_TLS_MODE` | `disabled` | `disabled`, `server`, `mutual` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OTLP gRPC endpoint (enables trace + metric export) |
| `OTEL_SERVICE_NAME` | — | Override service name in OTel metadata |
| `RUST_LOG` | `info` | Log level filter |
| `KMP_EVENT_STORE_BACKEND` | `valkey` | `valkey` or `nats` |

## Quick Pull

```bash
docker pull ghcr.io/underpass-ai/kmp:latest
```

## Quick Run

```bash
docker run --rm \
  -p 50054:50054 \
  -e KMP_GRAPH_URI=neo4j://host.docker.internal:7687 \
  -e KMP_DETAIL_URI=redis://host.docker.internal:6379 \
  -e KMP_SNAPSHOT_URI=redis://host.docker.internal:6379 \
  -e KMP_RUNTIME_STATE_URI=redis://host.docker.internal:6379 \
  -e NATS_URL=nats://host.docker.internal:4222 \
  ghcr.io/underpass-ai/kmp:latest
```

## Local Build

```bash
bash scripts/ci/container-image.sh ghcr.io/underpass-ai/kmp:dev-$(git rev-parse --short HEAD)
```

The script only builds — it does **not** push. Override runtime: `CONTAINER_RUNTIME=docker` or `CONTAINER_RUNTIME=podman`.

The build context must include `vendor/` because `Cargo.toml` patches
`async-nats` to `./vendor/async-nats`. Without that directory in the image
build context, `cargo build` fails inside the container.

## Push to GHCR

After building locally, push manually:

```bash
# Login (once)
echo "$(cat /tmp/github.txt)" | docker login ghcr.io -u USERNAME --password-stdin

# Push
docker push ghcr.io/underpass-ai/kmp:dev-$(git rev-parse --short HEAD)
```

Podman equivalent:

```bash
podman login ghcr.io -u USERNAME --password-stdin < /tmp/github.txt
podman push ghcr.io/underpass-ai/kmp:dev-$(git rev-parse --short HEAD)
```

## Deploy after push

```bash
# Via GitHub Actions (recommended)
gh workflow run deploy-kubernetes.yml \
  -f image_tag=dev-$(git rev-parse --short HEAD) \
  -f namespace=underpass-runtime \
  -f release=kmp

# Or locally
RELEASE_NAME=kmp \
NAMESPACE=underpass-runtime \
VALUES_FILE=charts/kmp/values.underpass-runtime.yaml \
IMAGE_TAG=dev-$(git rev-parse --short HEAD) \
bash scripts/ci/deploy-kubernetes.sh
```

## CI Publishing

Automated by [publish-distribution.yml](../../.github/workflows/publish-distribution.yml).

Triggers:
- push to `main` → tags: `latest`, `main`, `sha-<short>`
- push of tags matching `v*` → tags: `v0.1.0`, `sha-<short>`
- manual `workflow_dispatch`

When `GITHUB_TOKEN` cannot push to GHCR, set repo secrets:
- `GHCR_USERNAME`
- `GHCR_TOKEN`

## Helm Chart

The kernel ships a Helm chart at [`charts/kmp`](../../charts/kmp).
OCI target: `oci://ghcr.io/underpass-ai/charts/kmp`.

For Helm deployment, values, TLS, and observability stack configuration,
see [kubernetes-deploy.md](kubernetes-deploy.md).

Local chart validation:

```bash
bash scripts/ci/helm-lint.sh
```
