# kmp-ports

The command and query ports of [KMP by Underpass](https://github.com/underpass-ai/kmp),
the Kernel Memory Protocol kernel.

A thin crate on purpose: it names the operations the kernel drives its
adapters through — commands that change memory, queries that read it — and
re-exports the domain vocabulary those signatures speak. Adapters (embedded
redb, Neo4j, Valkey, NATS) implement these; the application layer composes
them.

Depending on this crate means depending on the shape of the kernel's
boundary, not on any implementation of it.

## Stability

Published so the rest of the kernel can be published, not as a curated public
API. It moves with the kernel's releases. Consumers embedding the kernel want
[`kmp-memory-api`](https://crates.io/crates/kmp-memory-api) instead.

## License

Apache-2.0.
