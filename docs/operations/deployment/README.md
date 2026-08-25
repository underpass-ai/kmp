# Docker and Kubernetes operations

The deployed topology is optional. It runs the same free and open-source KMP
protocol as a shared service; it is not a paid tier and it does not connect to
an Underpass-hosted memory service. Your organization operates the image,
storage, network, identity and observability boundaries.

Use it only when several clients need one live memory or when a central
availability, security and audit boundary is a real requirement. Otherwise,
[embedded KMP](../embedded/README.md) is the simpler path.

## What is shipped

| Artifact | Location | Contract |
|:--|:--|:--|
| OCI image | `ghcr.io/underpass-ai/kmp` | Built by [`Dockerfile`](../../../Dockerfile); contains `kmp-server`, `kmp-mcp-http` and the reference client. `kmp-server` is the entrypoint. |
| Helm chart | `oci://ghcr.io/underpass-ai/charts/kmp` | Source at [`distribution/charts/kmp`](../../../distribution/charts/kmp); deploys the kernel and optional in-chart infrastructure components. |

The server requires Neo4j, Valkey and NATS connection endpoints. Pulling the
image alone does not create those dependencies. This repository does not ship
a current Docker Compose stack.

The chart can deploy development instances of Neo4j, Valkey and NATS, or read
externally managed connection URIs from a Kubernetes Secret. These are
separate in-chart workloads, not sidecars in the kernel pod. Production
deployments should use an immutable image tag or digest and explicit storage,
backup, secret and certificate policies.

## Render before applying

The chart deliberately refuses an unspecified image and, outside explicit
development mode, inline connection URIs. Start from your own values file:

```bash
helm template kmp distribution/charts/kmp \
  --namespace kmp \
  --values <your-values.yaml> \
  --set image.tag=vX.Y.Z

helm upgrade --install kmp distribution/charts/kmp \
  --namespace kmp --create-namespace \
  --values <your-values.yaml> \
  --set image.tag=vX.Y.Z \
  --atomic --wait
```

Use [`values.yaml`](../../../distribution/charts/kmp/values.yaml) as the
configuration schema. The `values.underpass-runtime*.yaml` files are examples
for Underpass infrastructure, not portable defaults for another cluster.

## Boundaries

- The primary service is typed `KernelMemoryService` gRPC on port `50054`.
- Inbound gRPC supports disabled, server-TLS and mutual-TLS modes.
- NATS, Valkey and OTLP have explicit TLS configuration; Neo4j supports secure
  server schemes and CA trust, with the limitations stated in the
  [security model](../../security-model.md).
- The optional `kmp-mcp-http` deployment exposes authenticated Streamable HTTP
  MCP. Enabling it requires kernel mTLS plus explicit issuer, audience, origin
  and client-certificate configuration.
- The image and chart do not provide an authorization policy for direct gRPC
  callers. Put direct service access behind the boundary your organization
  controls.

An agent uses the deployed kernel by setting `KMP_KERNEL_GRPC_ENDPOINT` and,
when required, the `KMP_KERNEL_GRPC_TLS_*` variables on `kmp-mcp`. The same ten
MCP tools remain visible; changing the endpoint does not migrate a local
`.kernel/` store.

## Operational authority

Detailed prose runbooks were archived because they had drifted. Use the
executable sources below as the current contract:

| Need | Current source |
|:--|:--|
| Image contents and defaults | [`Dockerfile`](../../../Dockerfile) and [`publish-distribution.yml`](../../../.github/workflows/publish-distribution.yml) |
| Chart values and validation | [`values.yaml`](../../../distribution/charts/kmp/values.yaml) and [`templates/_helpers.tpl`](../../../distribution/charts/kmp/templates/_helpers.tpl) |
| Local chart validation | [`scripts/ci/helm-lint.sh`](../../../scripts/ci/helm-lint.sh) |
| Deployment automation | [`deploy-kubernetes.yml`](../../../.github/workflows/deploy-kubernetes.yml) and [`scripts/ci/deploy-kubernetes.sh`](../../../scripts/ci/deploy-kubernetes.sh) |
| Transport smoke | [`scripts/ci/kubernetes-transport-smoke.sh`](../../../scripts/ci/kubernetes-transport-smoke.sh) |
| Public HTTP MCP smoke | [`scripts/ci/mcp-http-live-smoke.sh`](../../../scripts/ci/mcp-http-live-smoke.sh) |
| Neo4j schema migration | `cargo run -p kmp-server --bin kmp-neo4j-migrate` |

The archived cluster and transport guides remain available for provenance at
[`archive/docs/operations/deployment`](../../../archive/docs/operations/deployment/),
but every command there must be revalidated before reuse.

For the supported topology and current limitations, continue with the
[enterprise overview](../../enterprise.md), [runtime guarantees](../../runtime-guarantees.md)
and [security model](../../security-model.md).
