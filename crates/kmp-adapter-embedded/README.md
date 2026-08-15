# kmp-adapter-embedded

The embedded-edition storage adapters of
[KMP by Underpass](https://github.com/underpass-ai/kmp), the Kernel Memory
Protocol kernel.

One `EmbeddedKernelStore` opens one data directory — a `FORMAT_VERSION` marker
next to `store/kernel.redb` — and implements every persistence port the kernel
needs: graph reads over materialized adjacency, node detail, the append-only
context event log, projection runtime state, snapshots and quality telemetry.
No server, no cluster, no daemon.

## What durability means here

Commits use redb's immediate durability, so the crash contract is explicit:
nothing is lost beyond the in-flight event, and replay applies nothing twice.
The store is single-writer per process — the kernel's own constraint, not an
accident of the engine.

## Not a special case

The observable semantics are pinned by the conformance suite that also
certifies the in-memory store and the Neo4j / Valkey adapters. A store that
passes it behaves like the others or it does not ship.

## License

Apache-2.0.
