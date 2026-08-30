# Architecture

This section is technical by design. It describes the executable boundaries
shared by the embedded and enterprise compositions; it does not describe the
archived product plans.

The current [architecture conformance audit](conformance-audit-2026-08-28.md)
checks SOLID, domain types, dependency direction and module ownership, then
orders the findings into small behaviour-preserving PRs. The
[kmp-mcp layer map](kmp-mcp-layer-map.md) executes its `kmp-mcp` findings for
#404: every file's target context and layer, and the slice order.

## System map

```mermaid
flowchart TB
    U[User] --> H[Agent host]
    P[Plugin] --> H
    S[Skills and host workflows] --> H
    H -->|MCP over local stdio| M[kmp-mcp]

    M -->|embedded calls| EK[Embedded kernel]
    EK --> ES[(SQLite)]
    EK --> V[Capability-guarded loopback viewer]

    M -->|remote gRPC| KS[KernelMemoryService]
    GW[Optional Streamable HTTP MCP gateway] -->|gRPC mTLS| KS
    C[Remote MCP client] -->|HTTPS and OIDC JWT| GW
    KS --> N[NATS JetStream]
    KS --> VK[(Valkey)]
    KS --> N4[(Neo4j)]
    KS -. OTLP .-> O[Operator collector]
```

The plugin is not a kernel adapter. It packages discovery and orchestration.
`kmp-mcp` is the protocol boundary. Both composition roots call the same
application use cases and expose the same thirteen MCP tools.

## Layer ownership

| Layer | Responsibility | Authoritative code |
|:--|:--|:--|
| Host plugin | Discovery, single MCP ownership, skills and human workflows | `plugins/kmp` |
| MCP adapter | JSON-RPC framing, schemas, pagination projection and backend selection | `crates/kmp-mcp` |
| Memory API | Versioned record and recall requests/views | `crates/kmp-memory-api` |
| Application | Ingest, wake, ask, temporal traversal, trace and inspect use cases | `crates/kmp-application` |
| Domain | Memory objects, relations, temporal coordinates and invariants | `crates/kmp-domain` |
| Ports | Persistence and projection interfaces | `crates/kmp-ports` |
| Embedded composition | In-process application plus local adapters | `crates/kmp-embedded`, `crates/kmp-adapter-embedded` |
| Enterprise composition | gRPC service plus Neo4j, Valkey and NATS adapters | `crates/kmp-server`, `crates/kmp-transport-grpc`, `crates/kmp-adapter-*` |
| HTTP gateway | OIDC authentication and tool-level authorization before gRPC forwarding | `crates/kmp-mcp-http` |

## Embedded write path

```mermaid
sequenceDiagram
    participant U as User
    participant A as Agent + skill
    participant M as kmp-mcp
    participant K as Embedded kernel
    participant S as SQLite
    participant B as .kmp/memory.jsonl

    U->>A: Remember a decision and its why
    A->>M: kmp_inspect(about, existing ref)
    M->>K: typed inspect query
    K->>S: read object, relations and evidence
    S-->>A: auditable prior context
    A->>M: kmp_write_memory(..., read_context)
    M->>M: validate intent and relation quality
    M->>K: canonical ingest
    K->>S: append event + synchronous projection
    S-->>K: durable commit
    K->>B: atomically publish project bundle
    K-->>A: accepted + read_after_write_ready
```

The commit-native bundle is maintained only for a project-scoped store. A
rejected validation does not create a partial write.

## Evidence-first read path

```mermaid
sequenceDiagram
    participant U as User
    participant A as Agent + skill
    participant M as kmp-mcp
    participant K as Kernel
    participant S as Store

    U->>A: Why did the rollback fail?
    A->>M: kmp_ask(about, question)
    M->>K: typed Ask query
    K->>S: retrieve direct evidence
    S-->>K: evidence + typed graph neighborhood
    K->>K: deterministic selection and byte-bounded projection
    K-->>M: refs, proof, evidence or UNKNOWN
    M-->>A: schema-checked response
    A-->>U: answer in the user's language
```

KMP does not generate the final answer. Direct evidence determines eligibility;
typed relations and their stored `why` can improve ordering and explain the
path. The agent generates conversational prose from the returned proof.

## Temporal model

Every recorded entry has an `observed_at` coordinate. Dimensions group entries
by task, process, episode or another explicit scope. Temporal reads use a
cursor and return visible pagination state; a partial page is never presented
as a complete interval.

`kmp_goto`, `kmp_near`, `kmp_rewind` and `kmp_forward` move through stored
time. `kmp_trace` and `kmp_inspect` audit graph structure and evidence. Semantic
Ask is not a substitute for temporal navigation.

## Backend selection

```mermaid
flowchart LR
    START[kmp-mcp starts] --> B{KMP_MCP_BACKEND}
    B -->|fixture| F[Reference fixtures; no persistence]
    B -->|grpc| R{Endpoint set?}
    R -->|no| FAIL[Fail fast]
    R -->|yes| G[gRPC kernel]
    B -->|embedded or unset| E[Embedded kernel]
    E --> D{Existing FORMAT_VERSION?}
    D -->|SQLite| X[Open SQLite]
    D -->|unsupported format| L[Reject untouched; use an archived exporter]
    D -->|no| Q[Fresh SQLite in shipped binary]
```

An endpoint may select gRPC when the backend is otherwise unspecified. Invalid
explicit combinations fail instead of silently opening a different store.

## Trust boundaries

| Boundary | Embedded | Enterprise |
|:--|:--|:--|
| Agent to MCP | Same-machine stdio | Same-machine stdio or remote HTTPS MCP |
| MCP to kernel | In-process call | gRPC with optional TLS/mTLS |
| User authorization | Agent host and local filesystem policy | OIDC scopes/resource grants at HTTP gateway; operator policy for direct gRPC |
| Persistence | Local filesystem | Neo4j, Valkey and NATS credentials and network policy |
| Human inspection | Loopback viewer with a random, process-lifetime capability | Operator-selected interfaces and telemetry |
| Failure mode | Store, format or lock errors | Network, identity, storage and projection failures in addition to domain errors |

## Deployment views

- [Embedded KMP](../embedded/README.md) defines the local composition.
- [Enterprise KMP](../enterprise/README.md) defines the remote composition.
- [Enterprise security](../enterprise/security.md) defines network and identity
  boundaries.
- [Enterprise observability](../enterprise/observability.md) defines the
  diagnostic boundary.

## Runbooks

- [MCP tools are missing](../runbooks/mcp-tools-missing.md)
- [Recover or move an embedded store](../runbooks/embedded-recovery.md)
- [Deploy or update enterprise KMP](../runbooks/enterprise-deployment.md)

Runbooks use the live CLI, chart and CI scripts as authority. If a command has
drifted, update the runbook in the same change that updates the executable
contract.
