# kmp-adapter-embedded

The embedded-edition storage adapters of
[KMP by Underpass](https://github.com/underpass-ai/kmp), the Kernel Memory
Protocol kernel.

One `EmbeddedKernelStore` opens one data directory — a `FORMAT_VERSION` marker
next to `store/` — and implements every persistence port the kernel needs:
graph reads over materialized adjacency, node detail, the append-only context
event log, projection runtime state, snapshots and quality telemetry. No
server, no cluster, no daemon.

## Two engines, one seam

The ports are written once against a small storage seam; the engine behind
it is chosen when the directory is created and recorded in `FORMAT_VERSION`,
so a store is never reopened by the wrong one.

| engine | `FORMAT_VERSION` | store file | build |
| --- | --- | --- | --- |
| redb (default) | 1 | `store/kernel.redb` | always |
| SQLite, WAL mode | 2 | `store/kernel.sqlite3` | `--features sqlite` |

redb is pure Rust, one file, one process: the default binary carries no C
toolchain and no store you already have changes. SQLite is the opt-in for the
case redb cannot serve — **two agent hosts open at once**, Claude Code and
Codex CLI on the same project — where readers never block the writer and a
second writer waits for the commit lock instead of being refused. Both engines
pass the same conformance suite, the same `kill -9` recovery test, and the
SQLite engine additionally passes a two-process, no-lost-events scenario that
redb is asserted to fail. See ADR-018.

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
