# kmp-application

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is its application layer.

Use cases, not infrastructure: ingest and write memory, wake an about, ask a
question against recovered evidence, go to a node, look around it, rewind and
move forward through time, trace what happened, inspect a node's detail and
the proof behind a relation.

Each service takes the ports it needs and nothing else — no client, no socket,
no environment. What composes them into a running kernel is a composition
root: [`kmp-embedded`](https://crates.io/crates/kmp-embedded) for the
in-process edition, the server binary for the deployed one.

Two properties this layer defends:

- **Idempotent writes.** Ingest and context updates report accepted versions
  and outcomes, so a retried call does not duplicate memory.
- **Answers made of evidence.** `ask` returns what the graph supports, or
  `UNKNOWN`. It does not generate.

## Stability

Published so the rest of the kernel can be published, not as a curated public
API. Consumers embedding the kernel want
[`kmp-memory-api`](https://crates.io/crates/kmp-memory-api).

## License

Apache-2.0.
