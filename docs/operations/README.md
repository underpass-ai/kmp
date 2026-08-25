# Operations

Operational docs for the kernel live here.

Current contents:

- [`deployment-boundary.md`](./deployment-boundary.md)
- [`container-image.md`](./container-image.md)
- [`kubernetes-deploy.md`](./kubernetes-deploy.md)
- [`kubernetes-transport-smoke.md`](./kubernetes-transport-smoke.md)
- [`cluster-prerequisites.md`](./cluster-prerequisites.md) — MetalLB, cert-manager, Route 53, NGINX Ingress
- [`mtls-deployment.md`](./mtls-deployment.md) — Full mTLS deployment guide
- [`neo4j-schema-migrations.md`](./neo4j-schema-migrations.md) — Explicit projection graph indexes and constraints
- [`mcp-stdio.md`](./mcp-stdio.md) — Stdio MCP adapter exposing the KMP tools
- [`mcp-http.md`](./mcp-http.md) — Public authenticated Streamable HTTP MCP adapter
- [`preflight.md`](./preflight.md) — E2E version preflight checklist before live runs
- [`embedded-hosts.md`](./embedded-hosts.md) — Claude Code, Codex and manual host wiring
- [`embedded-release.md`](./embedded-release.md) — prebuilt binaries, installer and store compatibility
- [`viewer.md`](./viewer.md) — local read-only memory graph and timeline

Related docs:

- [`../testing.md`](../testing.md) — Testing guide
- [`../observability.md`](../observability.md) — Observability stack
- [`../adr/ADR-007-quality-metrics-observability.md`](../adr/ADR-007-quality-metrics-observability.md) — Architecture decision

Key rule:

- this repo owns kernel operations and contract documentation
- this repo owns the standalone kernel image artifact
- this repo owns the standalone kernel Helm chart
- sibling repos may own runnable deployment packaging such as GHCR images,
  Docker Compose bundles, or Helm charts when those artifacts package a runtime
  or product layer rather than the kernel itself
