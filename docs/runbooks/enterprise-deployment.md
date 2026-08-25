# Runbook: deploy or update enterprise KMP

This runbook assumes authorization to change the target cluster. Use
`helm template` for a read-only review when that authority is absent.

## 1. Pin inputs

Record the release name and namespace, chart version, immutable image tag or
digest, values file, Secret names, rollback revision and dependency backup
status. Do not use `latest` outside explicit development mode.

## 2. Validate locally

```bash
bash scripts/ci/helm-lint.sh
helm template kmp distribution/charts/kmp \
  --namespace <namespace> \
  --values <values.yaml> \
  --set image.tag=<version>
```

Review the rendered image, Service, ingress, TLS modes, Secret references,
security contexts, connection URIs and optional HTTP MCP gateway.

## 3. Check external dependencies

Verify from the target network that Neo4j, Valkey and NATS are reachable with
the intended credentials and TLS policy. Verify the OTLP endpoint separately;
telemetry must not block memory, but an unreachable collector should be known
before rollout.

## 4. Server-side dry run

```bash
RELEASE_NAME=<release> \
NAMESPACE=<namespace> \
VALUES_FILE=<values.yaml> \
IMAGE_TAG=<version> \
DRY_RUN=true \
  bash scripts/ci/deploy-kubernetes.sh
```

Resolve every validation or admission error before mutation.

## 5. Deploy

```bash
RELEASE_NAME=<release> \
NAMESPACE=<namespace> \
VALUES_FILE=<values.yaml> \
IMAGE_TAG=<version> \
ATOMIC_DEPLOY=true \
WAIT_FOR_ROLLOUT=true \
  bash scripts/ci/deploy-kubernetes.sh
```

Use `IMAGE_DIGEST` instead of `IMAGE_TAG` when policy pins a digest. Never set
both.

## 6. Verify the release

```bash
kubectl -n <namespace> rollout status deployment/<server-deployment>
bash scripts/ci/kubernetes-transport-smoke.sh
```

When the HTTP MCP gateway is enabled, also run:

```bash
bash scripts/ci/mcp-http-live-smoke.sh
```

Confirm storage projections, one read/write lifecycle, TLS identity and the
configured telemetry path before switching agent endpoints.

## 7. Roll back or stop

With atomic deployment, a failed rollout should restore the prior Kubernetes
revision. Confirm it; do not assume dependency state was rolled back. If a
successful rollout later proves faulty, use the operator's reviewed Helm
rollback procedure and verify Neo4j, Valkey and NATS compatibility before
resuming writes.

Record the failed image, values and observed evidence. Fix forward with a new
immutable release; do not move an existing tag.
