# kmp-adapter-embedded

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate supplies its embedded
storage adapters.

One `EmbeddedKernelStore` opens one data directory — a `FORMAT_VERSION` marker
next to `store/` — and implements every persistence port the kernel needs:
graph reads over materialized adjacency, node detail, the append-only context
event log, projection runtime state, snapshots and quality telemetry. No
server, no cluster, no daemon.

## One active engine, one compatibility reader

The ports are written once against a small storage seam. Every new directory
is SQLite and records format 2; the legacy reader exists only so format-1
memory can still be opened and migrated.

| Engine | `FORMAT_VERSION` | Store file | Availability |
| --- | --- | --- | --- |
| SQLite, WAL mode | 2 | `store/kernel.sqlite3` | active; always available |
| legacy redb reader | 1 | `store/kernel.redb` | compatibility and migration only |

Two agent hosts can open the same project memory, readers do not block the
writer, and a second writer waits for the commit lock. Existing redb stores
remain readable from their format stamp and never change engine implicitly;
the public engine selector cannot create another one. SQLite passes the full
conformance, `kill -9` recovery and two-process no-lost-events suites. See
ADR-018.

Quality telemetry uses its own WAL journal at
`telemetry/quality.sqlite3`. An existing `quality.redb` is imported once,
bounded by the same retention policy and preserved on disk. redb remains a
private compatibility dependency for these format-1 reads; no active store,
telemetry writer or public engine option uses it.

## What durability means here

Canonical memory commits use SQLite `synchronous=FULL` in WAL, so the crash
contract is explicit: nothing is lost beyond the in-flight event, and replay
applies nothing twice. The bounded telemetry journal uses periodic FULL
durability and a final durable checkpoint.

## Not a special case

The observable semantics are pinned by the conformance suite that also
certifies the in-memory store and the Neo4j / Valkey adapters. A store that
passes it behaves like the others or it does not ship.

## License

Apache-2.0.
