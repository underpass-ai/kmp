# Development

This section is for contributors to KMP itself. Users installing local memory
should start with [Embedded KMP](../embedded/README.md).

- [KMP and ChronoLoom baseline](performance-baseline.md) — native reads, writes, real browser journeys and explicit measurement boundaries.
- [ChronoLoom scene performance](chronoloom-scene-performance.md) — retained rendering, interaction semantics and reproducible browser controls.
- [ChronoLoom asset delivery](chronoloom-asset-performance.md) — versioned precompressed browser assets, MCP App assembly and real HTTP controls.
- [Testing](testing.md) — the maintained local and CI verification paths.
- [Evidence-group acceptance controls](evidence-group-evaluation.md) — #538 source-backed multi-passage controls and their evaluation boundary.
- [Visual projection performance](visual-projection-performance.md) — snapshot-bound ChronoLoom reuse, correctness and reproducible measurements.
- [Releasing](releasing.md) — version, artifact and tag flow.
- [Agent-facing surface](agent-surface.md) — maintenance procedure for people and agents, including ownership, generation, validation and context cost.
- [Conditional argument schemas](agent-schema-conditions.md) — preserve callable fields and validate equivalent call forms against captured catalogues.
- [Guide examples](../../plugins/kmp/guide/README.md) — authored lessons and isolated MCP replays.
- [Semantic retrieval](semantic-retrieval.md) — optional retrieval policies and their contracts.
- [Automatic formation](automatic-formation.md) — source-grounded memory writing and verification.
- [Automatic entities](automatic-entities.md) — cited identity proposals and bounded expansion.
- [`api/proto`](../../api/proto/) — typed gRPC contract.
- [`api/asyncapi`](../../api/asyncapi/) — event projection contract.
- [`plugins/kmp/capabilities.json`](../../plugins/kmp/capabilities.json) —
  workflow and MCP ownership.

Historical E2E recipes, benchmark matrices and experimental integration guides
are archived. Use a current script under `scripts/ci/` as the command authority.
