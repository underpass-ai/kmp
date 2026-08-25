# Enterprise security

Enterprise KMP introduces network and identity boundaries that do not exist in
embedded mode. Security is an operator responsibility; the chart supplies
controls and validation, not a complete organizational policy.

## Kernel gRPC

The server supports `disabled`, `server` and `mutual` TLS modes through
`KMP_GRPC_TLS_*`. The Helm chart requires the corresponding Secret keys when a
TLS mode is enabled.

Use mTLS for clients that connect directly to `KernelMemoryService`, restrict
the Service or ingress to intended networks, and treat certificate identity as
transport authentication. The gRPC service does not implement a complete
end-user authorization backend.

## HTTP MCP gateway

The optional `kmp-mcp-http` gateway:

- requires an HTTPS public URL and HTTPS OIDC issuer;
- validates signed JWT issuer, audience, expiry, subject and key id;
- rejects symmetric JWT algorithms;
- enforces `kmp:read`, `kmp:write`, `kmp:inspect:raw` and
  `kmp:all-abouts` scopes as applicable;
- can restrict abouts, dimension scope ids and ref prefixes from token claims;
- restricts browser origins;
- applies request timeout and body-size limits;
- requires mutual TLS from the gateway to the kernel in the chart.

The relevant token claims are `scope`, `kmp_abouts`, `kmp_scope_ids` and
`kmp_ref_prefixes`. Issuing those claims correctly belongs to the operator's
identity system.

## Storage and event transports

- NATS supports server TLS or mTLS with explicit CA and client key paths.
- Valkey supports secure URIs, CA trust and an optional client certificate
  pair.
- Neo4j supports secure URI schemes and CA trust. The current Rust adapter
  does not configure client-certificate authentication.
- Credentials and certificate material belong in Kubernetes Secrets, not in
  committed values files.

## Pod defaults

The chart runs as a non-root UID/GID, disables privilege escalation, drops all
Linux capabilities, enables the runtime-default seccomp profile and uses a
read-only root filesystem with a writable temporary volume.

These defaults do not replace NetworkPolicy, Pod Security admission, secret
rotation, dependency backup or cluster policy.

## Authority

- [`crates/kmp-config`](../../crates/kmp-config/)
- [`crates/kmp-mcp-http/src/auth.rs`](../../crates/kmp-mcp-http/src/auth.rs)
- [`crates/kmp-mcp-http/src/authorization.rs`](../../crates/kmp-mcp-http/src/authorization.rs)
- [`distribution/charts/kmp/values.yaml`](../../distribution/charts/kmp/values.yaml)
- [`distribution/charts/kmp/templates/_helpers.tpl`](../../distribution/charts/kmp/templates/_helpers.tpl)
