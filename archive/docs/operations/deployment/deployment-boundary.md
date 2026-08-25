# Deployment Boundary

## Kernel Ownership

This repo owns:

- kernel source code (Rust crates)
- kernel contracts (gRPC proto + AsyncAPI)
- kernel integration examples and reference fixtures
- kernel quality gates (CI workflows, contract checks, coverage)
- kernel-focused documentation
- standalone kernel container image (`ghcr.io/underpass-ai/kmp`)
- standalone kernel Helm chart (`distribution/charts/kmp`)

The Helm chart includes **optional infrastructure sidecars** for self-contained
development and testing:

| Sidecar | Purpose | Default |
|:--------|:--------|:--------|
| Neo4j | Graph store | disabled |
| Valkey | Detail store + snapshots | disabled |
| NATS | Event bus | disabled |
| Loki | Log aggregation | disabled |
| Grafana | Dashboards | disabled |
| OTel Collector | Metrics export | disabled |

These are kernel-owned because the kernel cannot function without its
infrastructure. Packaging them as optional sidecars allows a single
`helm install` for development without depending on external infrastructure.

In production, consumers typically provide their own Neo4j, Valkey, and NATS.
`secrets.existingSecret` supplies the external connection URIs; the bundled
sidecars are disabled separately via `neo4j.enabled=false`,
`valkey.enabled=false`, and `nats.enabled=false`.

## This Repo Does NOT Own

- Docker Compose stacks for broader product systems
- Kubernetes deployment bundles for sibling runtimes or products
- Helm packaging for integrating runtimes
- Product-specific release pipelines
- Product-specific event bridges or adapters

## Why This Split Exists

Keeping deployment assets in the owning repo avoids:

- mixing kernel scope with runtime scope
- coupling kernel release cadence to product packaging
- reintroducing product-specific assumptions into the kernel repository

## Values Overlays

The chart ships values overlays for specific deployment targets:

| File | Target | Notes |
|:-----|:-------|:------|
| `values.yaml` | Base defaults | All sidecars disabled, no inline connections |
| `values.dev.yaml` | Local development | Inline connections allowed |
| `values.underpass-runtime.yaml` | `underpass-runtime` namespace | All sidecars enabled, observability stack enabled |
| `values.underpass-runtime.secure.example.yaml` | Production example | External infra with TLS, `secrets.existingSecret` |
| `values.underpass-runtime.mtls.example.yaml` | Full mTLS example | All boundaries mTLS except the in-chart Neo4j sidecar, which remains plaintext today; Grafana anonymous access disabled, OTLP TLS |

The `underpass-runtime` overlay enables all sidecars for a self-contained
deployment. The `secure.example` overlay shows production configuration with
external infrastructure and TLS — no sidecars, no inline credentials.

## Future Change Rule

Add deployment assets to this repo only if they package the kernel itself
as a standalone deliverable.

If the asset packages a runtime, a product adapter, or a broader system,
keep it in the owning sibling repo and link it from here instead.
