# Editions: embedded and cluster

KMP ships as **two editions of one protocol**. This page is the canonical
answer to "which one do I run, and what does it actually give me".

Everything below is about *how the kernel is hosted*. The memory model — abouts,
dimensions, temporal movement, typed relations, evidence — is identical in both,
and so is the tool surface. Nothing in the protocol is edition-specific.

## The short version

| | **Embedded edition** | **Cluster edition** |
|:--|:--|:--|
| Who it is for | one developer, one project | a team sharing and auditing memory |
| Kernel runs | in-process, inside the `kmp-mcp` binary | remote `KernelMemoryService` over gRPC |
| Storage | one local data dir (`.kernel/`; SQLite for fresh stores, existing redb stores remain supported) | Neo4j (graph) · Valkey (detail) · NATS JetStream (events) |
| Requires | nothing — no service, no database, no key | a deployed kernel plus TLS configuration |
| Projection | synchronous; `read_after_write_ready` always `true` | durable consumers; `true` on live ingest |
| Concurrency | SQLite supports concurrent local hosts; existing redb stores remain single-process | server-side |
| Transport security | no network surface | TLS / mTLS on gRPC, Valkey, NATS, OTLP |
| Observability | bounded local quality journal | OpenTelemetry + Loki |
| Deployment | `cargo install kmp-mcp` | Helm chart, container image |
| Select with | nothing — it is the default | `KMP_KERNEL_GRPC_ENDPOINT=…` |

`kmp-mcp` calls the cluster-edition path *live mode* in its own configuration
docs — same thing, seen from the adapter rather than from the deployment.

There is also a **fixture backend** (`KMP_MCP_BACKEND=fixture`) that returns
deterministic canned responses. It is for wiring an MCP client and validating
tool choice; it is not an edition and must be selected explicitly.

With no configuration the binary starts the embedded kernel. It fails fast on
an explicit invalid combination, such as requesting gRPC without an endpoint,
rather than guessing or opening a different store.

## Embedded edition

The kernel runs inside the MCP stdio process. There is no network surface, no
daemon, and no external dependency.

### Install and register

```bash
cargo install kmp-mcp
```

Host recipes — including which hosts are *tested on a real machine* versus
*derived from the host's documentation and pending verification* — are in
[embedded operations](operations/embedded/README.md). Claude Code and
Codex CLI are both tested. For those two, the
[KMP plugin](../plugins/kmp/README.md) performs the registration and adds the
agent-facing skill plus `/kmp:doctor`.

Prebuilt binaries and the one-command installer:
[embedded operations](operations/embedded/README.md).

### Where memory lives

Data-dir resolution, with the winning rule logged at startup (ADR-012):

1. `KMP_MCP_DATA_DIR`, if set;
2. otherwise the project `.kernel/` directory — the binary walks up from its
   working directory to the `.git` root and auto-gitignores the store;
3. otherwise `$XDG_DATA_HOME/kmp/default`.

Layout: `FORMAT_VERSION` (fail-fast on mismatch), `store/kernel.sqlite3` for a
fresh shipped store or `store/kernel.redb` for an existing redb store, `logs/`
(rotating JSON; stderr also, because stdout carries JSON-RPC only), and
`telemetry/quality.redb` (bounded, fail-open quality journal, ADR-014).

### What it guarantees

- **Durability.** Commits are fsync-durable.
- **Read-after-write.** Projection is synchronous, so a decision written in one
  tool call is recoverable in the next.
- **Parity.** The embedded backend reuses the live JSON path through shared
  proto mapping, and the conformance suite pins storage semantics across all
  backends in CI. Identical JSON is a tested claim.
- **Portability.** Project writes atomically maintain `.kmp/memory.jsonl` as a
  verifiable format-2 bundle; named snapshots provide immutable recovery
  points and read-only historical navigation. `kmp-mcp import` loads a bundle
  into an *empty* store, fail-fast.

### What it does not give you

- **Remote collaboration.** SQLite lets multiple local agent hosts share one
  data directory. It does not turn that directory into a remotely operated
  service for a team; use the Kubernetes edition when live access must cross
  machines or organizational boundaries.
- **Concurrent access to an existing redb store.** redb remains single-process
  by contract (ADR-011). A second session fails fast instead of corrupting
  memory. `kmp-mcp share-memory` snapshots, migrates and verifies it on SQLite
  when two local hosts need the same store.
- **Git sharing is deliberate.** The committed head and named snapshots can
  travel with git, be reviewed in a pull request, verified by digest and
  restored or read without replacing the live store. Uncommitted memory stays
  on the machine.
- **Transport security controls.** There is no transport. That is a feature
  here, not a gap — but it means there is nothing to configure and nothing to
  audit at the boundary.

### Watching a live session

Every embedded session serves a local read-only viewer over its own memory at
`http://127.0.0.1:7317/`, with nothing to install and nothing to configure: the
graph with typed relations, the note behind each node, a "known at" timeline
cursor, traces highlighted on the graph, and the exact rendered context a model
would receive with the hash covering it. Loopback only, non-local `Host`
headers refused, no authentication — see
[embedded operations](operations/embedded/README.md#local-data).

`KMP_VIEWER_ADDR` moves it to another address, and `off` declines it. If the
port is already taken — usually by another project's session — the one that
was offered the default steps aside and says so; a port is not worth a
session. An address you named yourself is a different matter, and a session
that cannot bind it fails rather than serving nothing quietly.

## Cluster edition

The typed `KernelMemoryService` runs as a deployed service. Adapters sit behind
ports so the protocol semantics can move toward backend-independent conformance
over time; today the deployment adapters are Neo4j, Valkey and NATS JetStream.

### Install

```bash
docker pull ghcr.io/underpass-ai/kmp:latest
```

`latest` is for a quick trial. Pin a `sha-<short-commit>` tag, a `v*` tag, or a
digest in production.

Deployment guide, values and TLS configuration:
[Docker and Kubernetes operations](operations/deployment/README.md).

### Verify a deployment

```bash
helm upgrade kmp distribution/charts/kmp --reuse-values --set e2e.enabled=true
helm test kmp --timeout 5m
```

The hooks cover transport and mTLS smoke plus the typed `KernelMemoryService`
lifecycle. Run `./scripts/e2e/regen.sh` first: it automates the version
preflight and reports stale binaries, drifted Helm state, missing certs, or
endpoint/model mismatches *before* the expensive tests run.

### What it guarantees

- **Shared, auditable memory** across people, agents and services.
- **Command safety.** Idempotency-key outcome recording plus optimistic
  concurrency (revision + content hash).
- **Projection integrity.** Durable pull consumers with explicit ack. If a
  handler fails, the message is nak'd and the runtime stops — an operator
  investigates rather than the system skipping ahead and leaving a hole.
- **Transport security.** gRPC supports plaintext, server TLS or mTLS; Valkey,
  NATS and OTLP can all take client certificates. Credentials are never
  inlined — always mounted from Kubernetes secrets.

### Current limits, stated plainly

- **Neo4j client-certificate auth is not available.** Server TLS and CA trust
  are supported; client-cert auth is limited by the Rust driver stack.
- **There is no authorization backend.** `ValidateScope` is set comparison
  only. Access control belongs to the caller today.
- Current behavior: [runtime guarantees](runtime-guarantees.md). Threat model
  and Helm TLS configuration: [security model](security-model.md).

## Moving between them

Switching is configuration. The same `kmp-mcp` binary talks to a deployed
kernel when you give it an endpoint:

```bash
KMP_KERNEL_GRPC_ENDPOINT=https://kernel.example.svc:50054 \
KMP_KERNEL_GRPC_TLS_MODE=mutual \
KMP_KERNEL_GRPC_TLS_CA_PATH=/var/run/kernel-tls/ca.crt \
KMP_KERNEL_GRPC_TLS_CERT_PATH=/var/run/kernel-tls/tls.crt \
KMP_KERNEL_GRPC_TLS_KEY_PATH=/var/run/kernel-tls/tls.key \
KMP_KERNEL_GRPC_TLS_DOMAIN_NAME=kmp-grpc \
  kmp-mcp
```

HTTPS endpoints enable server TLS with system/webpki roots automatically;
private CAs and mTLS are explicit, as above. Full matrix:
[embedded operations](operations/embedded/README.md).

**Moving the memory itself** is a separate step from moving the process.
`kmp-mcp export` produces a portable bundle from an embedded store; loading it
into a cluster deployment is an ingest problem, not a file copy. Plan it as a
migration, not as a config flip.

## Choosing

**Start embedded when** you are one developer or one team, the memory is about
a single project, and you want to know within ten minutes whether navigable
memory helps you at all. Nothing to operate, nothing to tear down.

**Move to the cluster when** memory has to be shared and audited across people
and services, when you need mTLS on every hop and traces landing in your own
collector, or when someone will eventually have to reconstruct a decision
without having been in the room.

That fork is a choice of operational weight, not of product. The contracts are
the same on both sides.
