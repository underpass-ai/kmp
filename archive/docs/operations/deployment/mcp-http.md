# Public Streamable HTTP MCP

The cluster edition can expose its existing `KernelMemoryService` at a public
MCP resource such as `https://kmp.underpassai.com/mcp`. The
`kmp-mcp-http` process is a stateless transport and authorization adapter. It
has no database, event consumer, ranker, projector, or LLM; every authorized
tool call reaches the same kernel over gRPC.

The KMP maintainers own the binary, OCI packaging, Helm resources, protocol
parity gate, and this runbook. The operator of the target cluster owns DNS,
the OAuth/OIDC authorization server, certificates, Kubernetes secrets, and
token grants.

## Public protocol

`POST /mcp` accepts one JSON-RPC message per request and returns JSON or `202`
for a notification. The current stateless MCP dialect is `2026-07-28`: clients
send matching `MCP-Protocol-Version`, `Mcp-Method`, and, for `tools/call`,
`Mcp-Name` headers, plus the required protocol version and client capabilities
in `params._meta`. KMP also accepts the headerless `2024-11-05` JSON-RPC shape
for existing clients. `GET` and `DELETE` on `/mcp` return `405`; KMP does not
mint an obsolete HTTP session id.

The adapter exposes the same ten definitions as stdio MCP:
`kmp_ingest`, `kmp_write_memory`, `kmp_wake`, `kmp_ask`, `kmp_goto`,
`kmp_near`, `kmp_rewind`, `kmp_forward`, `kmp_trace`, and `kmp_inspect`.
`kmp_write_memory` still compiles through the shared writer into canonical
`Ingest`; it is not a second write API.

Other endpoints are:

- `GET /healthz` and `GET /readyz`, available only inside the cluster;
- `GET /.well-known/oauth-protected-resource` and the path-specific variant,
  which publish RFC 9728 resource metadata.

Bodies default to 1 MiB and calls to 20 seconds. A present `Origin` must match
the exact allowlist. Missing or invalid bearer credentials return `401` with a
resource-metadata challenge; insufficient grants return `403` before a gRPC
call.

## Identity and grants

The gateway validates an asymmetric JWT against OIDC discovery/JWKS. It
requires `iss`, `aud`, `sub`, and `exp`, validates optional `nbf`, rejects HMAC
algorithms, and refreshes JWKS once for an unknown `kid`. Tokens are audience
bound to the MCP resource and are never forwarded to the kernel.

| Claim | Meaning |
|:------|:--------|
| `scope` | Space-delimited string or array. Reads need `kmp:read`; writes need `kmp:write`; raw inspect/raw temporal refs need `kmp:inspect:raw`; `all_abouts` needs `kmp:all-abouts`. |
| `kmp_abouts` | Exact about ids the request may name; `*` grants all. |
| `kmp_scope_ids` | Exact dimension scope ids the request may select; `*` grants all. |
| `kmp_ref_prefixes` | Prefixes allowed for Trace, Inspect, and cross-about writer links; `*` grants all. |
| `workspace` | Optional audit label. It does not expand authority. |

Authorization decisions log only subject, workspace, tool, about, decision,
and denial reason. Bearer tokens and request/evidence text are never logged.
`ValidateScope` is not involved: it remains a set comparison operation, not an
access-control boundary.

## Trust boundaries

```text
public client
  -- HTTPS + OAuth/OIDC bearer --> kmp-mcp-http
  -- gRPC + client mTLS --------> KernelMemoryService
  -- private service links -----> Neo4j / Valkey / NATS
```

The Helm chart renders a separate gateway Deployment, Service, HTTP Ingress,
and NetworkPolicy. Enabling it requires `tls.mode=mutual` on the kernel and a
client certificate secret for the gateway. The policy accepts ingress only
from the configured ingress namespace and limits egress to DNS, HTTPS for the
OIDC authority, and the release's kernel pods on the gRPC port.

Minimal production values:

```yaml
tls:
  mode: mutual
  existingSecret: kmp-grpc-server-tls

mcpHttp:
  enabled: true
  publicUrl: https://kmp.underpassai.com/mcp
  auth:
    issuer: https://identity.example.com/
    audience: https://kmp.underpassai.com/mcp
    allowedOrigins:
      - https://chatgpt.com
  grpcTls:
    existingSecret: kmp-mcp-grpc-client-tls
    domainName: kmp
  ingress:
    enabled: true
    className: nginx
    annotations:
      nginx.ingress.kubernetes.io/backend-protocol: HTTP
      nginx.ingress.kubernetes.io/ssl-redirect: "true"
    hosts:
      - host: kmp.underpassai.com
        paths:
          - path: /mcp
            pathType: Exact
          - path: /.well-known/oauth-protected-resource
            pathType: Prefix
    tls:
      - hosts: [kmp.underpassai.com]
        secretName: kmp-tls-prod
```

The OIDC issuer, audience, origins, and existing secret names are configuration,
not secret material. Private keys and bearer tokens belong only in Kubernetes
or CI secrets.

## Parity gate

Only HTTP/JSON-RPC/protobuf envelopes are transport-specific. Tool values,
ordering, errors, projection, and continuation are canonical KMP semantics.

| Execution path | Gate |
|:---------------|:-----|
| direct `KernelMemoryService` | `grpc_mcp_semantic_parity` invokes the live gRPC backend directly |
| embedded MCP | the same test seeds an embedded store with the identical canonical memory |
| stdio MCP → gRPC | the same test invokes `KernelMcpServer` over the live service |
| HTTP MCP → gRPC | the same test invokes the Axum router over that live service |

The matrix asserts exact results for all nine RPC moves and all ten MCP tools,
including writer dry-run compilation. Typed projector tests separately sweep
detail, byte limits, `max_entries`, page continuation/reconstruction, and stale
cursor errors; descriptor and tool-schema tests fail when the typed contract
or public MCP shape drifts.

The schema crosswalk classifies the existing compatibility-only fields rather
than leaving exceptions implicit: top-level `depth` maps to
`MemoryBudget.depth`, Trace `role` maps to `TraceRequest.goal`, and Inspect's
`budget.max_bytes` is transport-only refusal protection (it never changes
selection or truncates a result). JSON-RPC ids, MCP metadata and HTTP headers
are envelope-only. Any other semantic field must first exist in the typed
contract.

Run the local four-path gate with:

```bash
cargo test -p kmp-tests-kernel --features container-tests \
  --test mcp_real_kernel_integration grpc_mcp_semantic_parity -- --nocapture
```

After a deployment, run the public boundary and live gRPC comparison with
`scripts/ci/mcp-http-live-smoke.sh`. It requires a short-lived scoped token and
the existing gRPC client-mTLS files; the script neither prints nor persists the
token.

## Configuration

| Environment variable | Purpose |
|:---------------------|:--------|
| `KMP_MCP_HTTP_PUBLIC_URL` | HTTPS resource URL ending in `/mcp` |
| `KMP_MCP_HTTP_AUTH_ISSUER` | Exact HTTPS issuer URI |
| `KMP_MCP_HTTP_AUTH_AUDIENCE` | Required JWT audience |
| `KMP_MCP_HTTP_AUTH_JWKS_URI` | Optional explicit HTTPS JWKS endpoint; otherwise OIDC discovery is used |
| `KMP_MCP_HTTP_ALLOWED_ORIGINS` | Comma-separated exact HTTP(S) origins |
| `KMP_MCP_HTTP_ADDR` | Bind address; default `0.0.0.0:8080` |
| `KMP_MCP_HTTP_REQUEST_TIMEOUT_SECS` | Per-call deadline; default `20`, maximum `300` |
| `KMP_MCP_HTTP_MAX_BODY_BYTES` | Body ceiling; default `1048576` |
| `KMP_MCP_HTTP_REQUIRE_GRPC_MTLS` | Defaults to `true`; only disable for local tests |

The usual `KMP_KERNEL_GRPC_*` variables select the remote kernel and its client
mTLS identity. Production startup fails unless the selected backend is gRPC
and its TLS mode is `mutual`.
