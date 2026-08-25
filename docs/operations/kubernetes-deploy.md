# Kubernetes Deploy

## Purpose

Document the manual deployment workflow for the standalone kernel Helm chart.

## Workflow

- workflow: [deploy-kubernetes.yml](../../.github/workflows/deploy-kubernetes.yml)
- trigger: `workflow_dispatch`

The workflow deploys the chart in
[`charts/kmp`](../../charts/kmp) with
`helm upgrade --install`.

## Required Secret

The workflow expects this repository secret:

- `KUBECONFIG_B64`

The value must be a base64-encoded kubeconfig file.

Example:

```bash
base64 -w0 ~/.kube/config
```

## Inputs

- `namespace`
- `release`
- `values_file`
- `image_tag`
- `image_digest`
- `helm_timeout`
- `dry_run`
- `wait_for_rollout`
- `atomic`

Set either `image_tag` or `image_digest`.

## Default Target

The default values file is:

- [`charts/kmp/values.underpass-runtime.yaml`](../../charts/kmp/values.underpass-runtime.yaml)

The chart deploys all infrastructure self-contained within the target namespace:
Neo4j, Valkey, and NATS are managed as in-chart optional components (`neo4j.enabled`,
`valkey.enabled`, `nats.enabled`). Connection URIs are auto-generated from the
release name. There are no cross-namespace dependencies.

The secure follow-up profile is:

- [`charts/kmp/values.underpass-runtime.secure.example.yaml`](../../charts/kmp/values.underpass-runtime.secure.example.yaml)

That file captures the intended target once TLS is enabled across all
connections.

## Local Equivalent

The workflow delegates to:

- [`scripts/ci/deploy-kubernetes.sh`](../../scripts/ci/deploy-kubernetes.sh)

`IMAGE_TAG` refers to a tag on `ghcr.io/underpass-ai/kmp`.
CI publishes `main`, `latest`, `sha-<short>`, and `v*` tags automatically.
For development, build and push a `dev-<sha>` tag first (see [container-image.md](container-image.md)).

```bash
# Deploy the latest CI build
RELEASE_NAME=kmp \
NAMESPACE=underpass-runtime \
VALUES_FILE=charts/kmp/values.underpass-runtime.yaml \
IMAGE_TAG=main \
bash scripts/ci/deploy-kubernetes.sh
```

Development cycle (local changes):

```bash
# 1. Build + push
bash scripts/ci/container-image.sh ghcr.io/underpass-ai/kmp:dev-$(git rev-parse --short HEAD)
docker push ghcr.io/underpass-ai/kmp:dev-$(git rev-parse --short HEAD)

# 2. Deploy
RELEASE_NAME=kmp \
NAMESPACE=underpass-runtime \
VALUES_FILE=charts/kmp/values.underpass-runtime.yaml \
IMAGE_TAG=dev-$(git rev-parse --short HEAD) \
bash scripts/ci/deploy-kubernetes.sh

# 3. Run E2E benchmark against the cluster
EVAL_MATRIX_PATH=smoke-run.yaml FILTER_SCALES=micro \
cargo test -p kmp-tests-paper --features container-tests \
  --test vllm_benchmark_integration -- --nocapture --test-threads=1
```

For an already configured live release, prefer preserving live values instead
of reapplying the default values file. This avoids accidentally dropping mTLS,
Ingress, `imagePullSecrets`, or cluster-specific secrets:

```bash
helm upgrade --install kmp charts/kmp \
  -n underpass-runtime \
  --reuse-values \
  --set image.tag=dev-$(git rev-parse --short HEAD) \
  --set image.digest= \
  --wait \
  --timeout 10m
```

To force a restart with the same image tag, add a simple pod annotation:

```bash
helm upgrade --install kmp charts/kmp \
  -n underpass-runtime \
  --reuse-values \
  --set image.tag=dev-$(git rev-parse --short HEAD) \
  --set image.digest= \
  --set-json 'podAnnotations={"redeployAt":"<timestamp>"}' \
  --wait \
  --timeout 10m
```

Use a plain annotation key for `podAnnotations` unless you are deliberately
escaping dotted annotation names for Helm. Mis-escaped dotted keys can render a
nested object where Kubernetes expects string annotations.

## Smoke Validation

After deploying transport-security changes, use the dedicated smoke script:

- [`scripts/ci/kubernetes-transport-smoke.sh`](../../scripts/ci/kubernetes-transport-smoke.sh)
- runbook: [kubernetes-transport-smoke.md](./kubernetes-transport-smoke.md)

That path validates real in-cluster gRPC TLS and mTLS, and it can also validate
outbound NATS and Valkey TLS against a TLS-enabled environment.

After deploying graph-explorer changes, use the starship explorer end-to-end test:

- [`crates/kmp-transport-grpc/src/starship_e2e.rs`](../../crates/kmp-transport-grpc/src/starship_e2e.rs)

That path validates the explorer journey itself: root load, node detail,
mid-level zoom, and leaf rehydration against the deployed release.

## gRPC TLS and mTLS

The chart now exposes inbound gRPC transport mode directly:

- `tls.mode=disabled`
- `tls.mode=server`
- `tls.mode=mutual`

When `tls.mode` is `server` or `mutual`, set:

- `tls.existingSecret`
- optionally `tls.mountPath`
- secret keys under `tls.keys.*`

Expected secret data:

- `tls.crt`: server certificate
- `tls.key`: server private key
- `ca.crt`: client CA certificate for `tls.mode=mutual`

Example:

```bash
kubectl create secret generic kmp-grpc-tls \
  -n underpass-runtime \
  --from-file=tls.crt=server.crt \
  --from-file=tls.key=server.key \
  --from-file=ca.crt=client-ca.crt
```

Example values override:

```yaml
tls:
  mode: mutual
  existingSecret: kmp-grpc-tls
```

## Ingress Exposure

The chart can optionally render a Kubernetes `Ingress` in front of the gRPC
service.

The core values are:

- `ingress.enabled`
- `ingress.className`
- `ingress.annotations`
- `ingress.hosts`
- `ingress.tls`

Example for an NGINX ingress controller serving gRPC:

```yaml
ingress:
  enabled: true
  className: nginx
  annotations:
    nginx.ingress.kubernetes.io/backend-protocol: GRPC
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "30"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "300"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "300"
    nginx.ingress.kubernetes.io/proxy-body-size: 10m
  hosts:
    - host: kmp.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - hosts:
        - kmp.example.com
      secretName: kmp-ingress-tls
```

Controller-specific annotations stay with the operator because gRPC ingress
behavior is controller-specific.

### Public TLS Termination With NGINX

This is the current `underpass-runtime` exposure profile.

Use this when the kernel pod listens with `tls.mode=disabled` and NGINX is the
public TLS boundary:

- external clients call `https://kmp.underpassai.com`
- NGINX terminates the public certificate from `ingress.tls`
- NGINX forwards gRPC to the kernel service using plaintext upstream h2c
- the required backend annotation is
  `nginx.ingress.kubernetes.io/backend-protocol: GRPC`

Do not use `backend-protocol: GRPCS` in this profile. `GRPCS` means NGINX must
open a TLS connection to the kernel backend, which requires the kernel server
itself to run with `tls.mode=server` or `tls.mode=mutual`.

The sibling runtime profile enables this directly with:

- host: `kmp.underpassai.com`
- class: `nginx`
- annotation: `nginx.ingress.kubernetes.io/backend-protocol: GRPC`
- TLS secret: `kmp-tls-prod`
- NGINX gRPC timeouts: 30s connect and 300s read/send, so synchronous
  `kmp_write_memory` commits with `read_after_write_ready` are not cut at the
  controller default 60s timeout.

### GRPCS Upstream To Kernel

Use `backend-protocol: GRPCS` only when NGINX should connect to a TLS-enabled
kernel backend.

For server-side TLS, configure:

- `tls.mode=server`
- `tls.existingSecret`
- `nginx.ingress.kubernetes.io/backend-protocol: GRPCS`

For upstream mTLS, configure the same backend TLS plus the NGINX client identity
annotations:

- `tls.mode=mutual`
- `nginx.ingress.kubernetes.io/proxy-ssl-secret`
- `nginx.ingress.kubernetes.io/proxy-ssl-name`
- `nginx.ingress.kubernetes.io/proxy-ssl-verify`

That is a transport-security profile, not the default public exposure profile.
See [mtls-deployment.md](./mtls-deployment.md) for the full mTLS deployment.

## Outbound NATS TLS

The chart now exposes NATS client TLS directly:

- `natsTls.mode=disabled|server|mutual`
- `natsTls.existingSecret`
- `natsTls.mountPath`
- `natsTls.keys.ca`
- `natsTls.keys.cert`
- `natsTls.keys.key`
- `natsTls.tlsFirst`

Expected secret data when using private trust or client identity:

- `ca.crt`: CA certificate for the NATS server
- `tls.crt`: client certificate for `natsTls.mode=mutual`
- `tls.key`: client private key for `natsTls.mode=mutual`

Example:

```bash
kubectl create secret generic kmp-nats-tls \
  -n underpass-runtime \
  --from-file=ca.crt=nats-ca.crt \
  --from-file=tls.crt=client.crt \
  --from-file=tls.key=client.key
```

Example values override:

```yaml
natsTls:
  mode: mutual
  existingSecret: kmp-nats-tls
  tlsFirst: true
  keys:
    ca: ca.crt
    cert: tls.crt
    key: tls.key
```

`natsTls.mode=server` may also be used without a mounted secret when system
trust roots are sufficient.

## Outbound Valkey TLS

The chart now exposes Valkey TLS material directly:

- `valkeyTls.enabled=true|false`
- `valkeyTls.existingSecret`
- `valkeyTls.mountPath`
- `valkeyTls.keys.ca`
- `valkeyTls.keys.cert`
- `valkeyTls.keys.key`

Expected secret data when using private trust or client identity:

- `ca.crt`: CA certificate for Valkey
- `tls.crt`: client certificate for mutual TLS
- `tls.key`: client private key for mutual TLS

Example:

```bash
kubectl create secret generic kmp-valkey-tls \
  -n underpass-runtime \
  --from-file=ca.crt=valkey-ca.crt \
  --from-file=tls.crt=client.crt \
  --from-file=tls.key=client.key
```

Inline connection values are rewritten automatically from `redis://` or
`valkey://` to `rediss://` or `valkeys://`, and Helm appends the configured
`tls_*_path` query parameters using the mounted file paths.

Example values override:

```yaml
valkeyTls:
  enabled: true
  existingSecret: kmp-valkey-tls
  keys:
    ca: ca.crt
    cert: tls.crt
    key: tls.key
```

If you use `secrets.existingSecret` for connection URIs, Helm cannot rewrite
the secret-backed values. In that case, store the final `rediss://` or
`valkeys://` URIs in the secret, including `tls_ca_path`, `tls_cert_path`, and
`tls_key_path` query parameters that match the mounted paths.

## Neo4j TLS Today

The Neo4j adapter supports secure schemes such as:

- `bolt+s://`
- `bolt+ssc://`
- `neo4j+s://`
- `neo4j+ssc://`

The chart now supports first-class mounting of a custom Neo4j CA for inline
`connections.graphUri` values.

The self-contained Neo4j serves over plaintext `neo4j://` by default. The
staged secure target lives in
[`charts/kmp/values.underpass-runtime.secure.example.yaml`](../../charts/kmp/values.underpass-runtime.secure.example.yaml)
and switches the graph connection to `neo4j+s://...` plus `neo4jTls.*`.

Example values override:

```yaml
neo4jTls:
  enabled: true
  existingSecret: kmp-neo4j-tls
  keys:
    ca: ca.crt

connections:
  graphUri: bolt+s://neo4j.example.internal:7687
```

That renders `KMP_GRAPH_URI` with the matching `tls_ca_path` query
parameter that points at the mounted CA file.

Current boundary:

- publicly trusted Neo4j certificates can be handled through the base image
  trust store plus a secure `graphUri`
- private-trust Neo4j deployments can use `neo4jTls.*` with inline
  `connections.graphUri`
- secret-backed `graphUri` values still need the final secure URI, including
  `tls_ca_path`, to be stored in the secret because Helm cannot rewrite secret
  contents
- Neo4j client identity is still outside the current chart surface

## Observability Stack

The chart includes optional Loki, Grafana, and OTel Collector in-chart components:

```yaml
loki:
  enabled: true

grafana:
  enabled: true
  adminPassword: admin  # change in production

otelCollector:
  enabled: true
```

When `otelCollector.enabled=true`, the kernel deployment automatically sets
`OTEL_EXPORTER_OTLP_ENDPOINT` to the in-chart collector service alias
`<release>-otel`. Helm uses
`http://...:4317` in plaintext mode and `https://...:4317` when
`otelCollector.tls.enabled=true`.
For the in-chart collector, traces are disabled by default via
`OTEL_TRACES_EXPORTER=none` because the bundled collector config only routes
metrics and optional Loki-bound logs.

The OTel Collector exports:
- **Metrics** → Prometheus endpoint (always active when collector is enabled)
- **Logs** → Loki push API (only when `loki.enabled=true` — if Loki is disabled,
  the collector's logs pipeline is not configured)

Grafana auto-provisions datasources:
- **Loki** — structured log queries via LogQL (requires `loki.enabled=true`)
- **Prometheus** — OTel metrics via PromQL (requires `otelCollector.enabled=true`)

Access Grafana:
```bash
kubectl port-forward svc/<release>-grafana 3000:3000 -n <namespace>
```

See [observability.md](../observability.md) for metric definitions, LogQL/PromQL examples,
and architecture details.

### Full mTLS Deployment

For a deployment with mutual TLS on supported boundaries:

```bash
helm upgrade --install kmp charts/kmp \
  -n underpass-runtime \
  -f charts/kmp/values.underpass-runtime.mtls.example.yaml \
  --set image.tag=<your-tag>
```

This configures:
- gRPC inbound mTLS (client cert validation)
- NATS mTLS (kernel ↔ NATS)
- Valkey mTLS (kernel ↔ Valkey)
- Neo4j TLS with CA trust (mTLS pending neo4rs driver upgrade)
- OTel Collector mTLS (receiver + Loki exporter)
- Kernel → OTel Collector mTLS (via `OTEL_EXPORTER_OTLP_*_PATH` env vars that
  point to the mounted OTel TLS secret)
- Grafana with anonymous access disabled

Requires secrets pre-created in the namespace. See
[`values.underpass-runtime.mtls.example.yaml`](../../charts/kmp/values.underpass-runtime.mtls.example.yaml)
for the full values file with secret names and cert paths.

## Notes

- `dry_run=true` uses `helm --dry-run=server`
- `atomic=true` is skipped automatically during dry-run
- cluster-specific secrets such as `imagePullSecrets` remain chart/value
  concerns, not workflow concerns
