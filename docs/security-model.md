# Security Model

## Product Security Direction

The queryable memory layer must be observable, auditable, and secure.

Security for agentic memory is not only transport encryption. The kernel stores
process memory that may include user context, agent decisions, tool outputs,
evidence, and raw operational details. Callers should be able to inspect what
they are authorized to inspect, and failures should be explicit rather than
hidden behind fallback context.

Security goals:

- protect every transport boundary with TLS or mTLS where available;
- keep credentials in secrets, not inline configuration;
- fail closed on unauthorized or ambiguous access once application identity is
  introduced;
- separate compact usable responses from raw inspect access;
- redact or withhold sensitive details when a caller lacks permission;
- emit audit logs for memory writes, reads, trace, inspect, and rejected access;
- avoid leaking secrets, raw prompts, or unrestricted memory in logs and traces.

Medium-term gaps:

- application-level caller identity is not extracted from mTLS certificates;
- fine-grained RBAC and scope-based access control are not implemented;
- raw inspect permission is not a separate authorization decision yet;
- redaction policy is not a first-class KMP concept yet;
- data-at-rest encryption is delegated to infrastructure.

## Transport Security Posture

Every infrastructure boundary supports TLS. The gRPC transport supports
mutual TLS (mTLS). OTLP export to the OTel Collector supports TLS and mTLS when
the OTLP endpoint and certificate paths are configured.

```mermaid
graph LR
    C[Callers] -- gRPC + mTLS --> K[kmp]
    K -- TLS --> N4[(Neo4j)]
    K -- TLS + mTLS --> VK[(Valkey)]
    K -- TLS --> NT[(NATS)]
    K -- TLS + mTLS --> OT[OTel Collector]
```

> For the in-chart collector, Helm auto-wires the OTLP endpoint to the
> `<release>-otel` service alias and uses
> `https://...:4317` when `otelCollector.tls.enabled=true`. The kernel still
> needs `OTEL_EXPORTER_OTLP_{CA,CERT,KEY}_PATH` to point at the mounted client
> certificate material.

## Trust Boundaries

### Boundary 1: gRPC Transport → Kernel

- **TLS**: Full support for server TLS and mutual TLS via
  `KMP_GRPC_TLS_MODE` (`disabled`, `server`, `mutual`).
- **mTLS**: When `mode=mutual`, the kernel validates client certificates
  against a trusted CA. Only callers presenting a valid certificate are
  accepted. Configured via `tls.existingSecret` and `tls.keys.clientCa`
  in the Helm chart.
- **Authentication**: mTLS provides transport-level identity. No
  application-level identity extraction in v1beta1.
- **Authorization**: Not enforced in v1beta1. `ValidateScope` is a
  set-comparison utility, not an access control gate.

### Boundary 2: Kernel → Neo4j

- **Transport**: Supports `bolt+s://` and `neo4j+s://` (TLS) with private
  CA trust via `neo4jTls.existingSecret` in the Helm chart. The CA
  certificate is mounted as a volume and referenced via `tls_ca_path`
  in the connection URI.
- **Authentication**: URI-embedded credentials. Must be provided via
  Kubernetes secrets in production.
- **Client mTLS**: Neo4j supports mTLS since v5.19+ (client cert as 2FA).
  Helm chart and URI parsing support `tls_cert_path` + `tls_key_path`.
  Pending: `neo4rs` 0.8 `ConfigBuilder` only exposes `with_client_certificate`
  for CA — passing the client cert to the driver requires upgrade or manual
  rustls config.
- **Authorization**: Single Neo4j connection identity per kernel instance.

### Boundary 3: Kernel → Valkey

- **Transport**: Supports `rediss://` (TLS) with mutual TLS via
  `valkeyTls.*` Helm values. Client certificate and key mounted from
  secrets for mTLS authentication.
- **Authentication**: TLS client identity or URI-embedded credentials.
- **Data at rest**: Valkey does not encrypt at rest by default. Snapshots,
  node details, and event store entries are stored as JSON.

### Boundary 4: Kernel → NATS

- **Transport**: TLS via `natsTls.*` Helm values. Supports CA pinning,
  client certificates, and `tls_first` for NATS TLS-first mode.
- **Authentication**: NATS connection credentials or TLS client identity.
- **Authorization**: Subject-level permissions managed in NATS server
  configuration, not in the kernel.

## Threat Model (v1beta1)

| Threat | Mitigation | Status |
|--------|-----------|--------|
| Unauthenticated gRPC access | Mutual TLS | **Implemented** |
| Man-in-the-middle on gRPC | Server TLS / mTLS | **Implemented** |
| Man-in-the-middle on Neo4j | TLS with CA pinning | **Implemented** |
| Man-in-the-middle on Valkey | TLS with mTLS | **Implemented** |
| Man-in-the-middle on NATS | TLS with CA pinning | **Implemented** |
| Unauthorized context reads | mTLS restricts callers; no fine-grained RBAC | Partial |
| Unauthorized context writes | mTLS + atomic CAS (JetStream sequence / Valkey Lua) | Yes |
| Replay attacks on commands | Idempotency key outcome recording (returns same result for retries; application layer checks before processing) | **Implemented** |
| Credential exposure in config | Kubernetes secrets, not inline URIs | **Documented** |
| Data exfiltration from backends | TLS transport, network isolation | **Available** |
| Man-in-the-middle on OTLP | mTLS via `OTEL_EXPORTER_OTLP_{CA,CERT,KEY}_PATH` env vars | **Implemented** |
| Grafana anonymous access | Helm default: anonymous=false. Dev overlay enables it | Configurable via `grafana.anonymousAccess` |
| Unauthenticated public MCP access | OAuth/OIDC JWT validation, audience binding, RFC 9728 challenge | **Implemented in `kmp-mcp-http`** |
| Cross-tenant public MCP read/write | Explicit token grants for scopes, abouts, dimension scope ids, and ref prefixes; deny before gRPC | **Implemented in `kmp-mcp-http`** |
| Raw inspect exposure | Separate `kmp:inspect:raw` permission | **Implemented in `kmp-mcp-http`** |
| Public adapter bypasses kernel mTLS | Helm validation and gateway startup require mutual TLS | **Implemented** |


## Helm TLS Configuration Summary

| Component | Helm Values | Modes |
|-----------|------------|-------|
| gRPC transport | `tls.mode`, `tls.existingSecret` | disabled, server, mutual |
| Neo4j | `neo4jTls.enabled`, `neo4jTls.existingSecret`, `neo4jTls.keys.{ca,cert,key}` | TLS with CA trust + client cert (driver pending) |
| Valkey | `valkeyTls.enabled`, `valkeyTls.existingSecret` | TLS, mTLS with client cert |
| NATS | `natsTls.mode`, `natsTls.existingSecret` | TLS, mTLS, tls_first |
| OTel Collector | `otelCollector.tls.enabled`, `otelCollector.tls.existingSecret` | mTLS (receiver + Loki exporter + kernel OTLP client) |
| Public MCP adapter | `mcpHttp.*`, `mcpHttp.grpcTls.existingSecret` | OAuth/OIDC externally; gRPC mTLS internally |

## Recommendations for Production

1. Set `tls.mode=mutual` for gRPC transport.
2. Enable `neo4jTls`, `valkeyTls`, and `natsTls` for all backend connections.
3. Use `secrets.existingSecret` for connection URIs — never inline credentials.
4. Restrict NATS subject permissions to kernel-owned prefixes.
5. Network-isolate the kernel namespace from untrusted workloads.
6. Set `grafana.adminPassword` to a strong value. Anonymous access is disabled
   by default (`grafana.anonymousAccess=false`); only the dev overlay enables it.
7. Set `OTEL_EXPORTER_OTLP_CA_PATH`, `_CERT_PATH`, `_KEY_PATH` for OTLP mTLS.
8. If public MCP is enabled, issue short-lived audience-bound tokens with the
   smallest `kmp:*`, about, dimension-scope, and ref-prefix grants.

## What the Kernel Does NOT Do

- Does not extract application-level caller identity from mTLS certificates.
- The typed gRPC kernel does not enforce application RBAC. The optional public
  HTTP MCP adapter enforces its documented token grants before gRPC.
- Does not encrypt data at rest.
- Does not manage secrets or rotate credentials.
- Does not validate the truthfulness of application-supplied explanations.
