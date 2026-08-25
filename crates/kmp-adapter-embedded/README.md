# kmp-adapter-embedded

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate supplies its embedded
storage adapters.

One `EmbeddedKernelStore` opens one data directory — a `FORMAT_VERSION` marker
next to `store/` — and implements every persistence port the kernel needs:
graph reads over materialized adjacency, node detail, the append-only context
event log, projection runtime state, snapshots and quality telemetry. No
server, no cluster, no daemon.

## Two engines, one seam

The ports are written once against a small storage seam; the engine behind
it is chosen when the directory is created and recorded in `FORMAT_VERSION`,
so a store is never reopened by the wrong one.

| Engine | `FORMAT_VERSION` | Store file | Availability |
| --- | --- | --- | --- |
| SQLite, WAL mode | 2 | `store/kernel.sqlite3` | `sqlite` feature; enabled by default in `kmp-mcp` |
| redb compatibility | 1 | `store/kernel.redb` | always |

SQLite is the engine for a fresh user-facing `kmp-mcp` store: two agent hosts
can open the same project memory, readers do not block the writer, and a
second writer waits for the commit lock. The crate keeps SQLite feature-gated
so a library consumer can still choose a pure-Rust build. Existing redb stores
remain readable from their format stamp and never change engine implicitly.
Both engines pass the same conformance and `kill -9` recovery suites; SQLite
also passes a two-process, no-lost-events scenario. See ADR-018.

A binary built without the feature still recognises a SQLite store and
refuses it by name, saying which feature to enable; a binary older than the
layout refuses it as "newer than this binary supports". Neither ever opens an
empty store beside it.

## What durability means here

Commits are fsync-durable on both engines (redb immediate durability; SQLite
`synchronous=FULL` in WAL), so the crash contract is explicit: nothing is lost
beyond the in-flight event, and replay applies nothing twice.

## Not a special case

The observable semantics are pinned by the conformance suite that also
certifies the in-memory store and the Neo4j / Valkey adapters. A store that
passes it behaves like the others or it does not ship.

## License

Apache-2.0.
