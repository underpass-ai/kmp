# Development

This section is for contributors to KMP itself. Users installing local memory
start with [Embedded KMP](../embedded/README.md).

Everything listed here describes how KMP works and is checked today. Dated
records of finished work (increment notes, acceptance matrices, issue
evidence) are in the [archive](../archive/README.md).

## Contributing and testing

- [Testing](testing.md) — local checks, CI routing, coverage floors, embedded
  and infrastructure gates, scratch and evidence rules.
- [Contributing](../../CONTRIBUTING.md) — the working agreement for changes.

## Releases and distribution

- [Releasing](releasing.md) — version, candidate, tag and marketplace flow.
- [Distribution channels](distribution-channels.md) — every storefront, what
  it reads from the tree, how it updates and what is submitted by hand.

## The agent surface

- [Agent-facing surface](agent-surface.md) — maintenance procedure: where each
  part of the surface is owned, how it is generated, validated and measured.
- [Conditional argument schemas](agent-schema-conditions.md) — the
  cross-field rules the validator enforces and `tools/list` no longer
  publishes.
- [Guide examples](../../plugins/kmp/guide/README.md) — authored lessons and
  isolated MCP replays.
- [`plugins/kmp/capabilities.json`](../../plugins/kmp/capabilities.json) —
  workflow and MCP ownership.
- [`api/proto`](../../api/proto/) — typed gRPC contract (experimental, outside
  the 1.0 promise).
- [`api/asyncapi`](../../api/asyncapi/) — event projection contract.

## Retrieval and Jev internals

- [Retrieval configuration](../embedded/configuration.md) — defaults and
  optional integrations, as users see them.
- [Lexical index](lexical-sidecar.md) — the default local index, eligibility
  limits, lazy pages and measured costs.
- [Evidence re-ranking](evidence-rerank.md) — opt-in TypeSafe Jev judgments
  for Ask, Wake and relation proposals.
- [TypeSafe Jev with KMP](jev-evaluation.md) — judged corpus, recorded
  cassette and floors for curate, the pre-write check and Ask re-ranking.
- [Semantic retrieval](semantic-retrieval.md) — the experimental local encoder
  channel and its contracts.
- [Shared passages and composed contexts](context-projection.md) —
  `KMP_MCP_PASSAGES=shared` and `kmp-mcp context`.
- [Incremental consolidated views](incremental-consolidation.md) —
  `kmp-mcp consolidation`.

## Benchmarks and gate inputs

- [Memory bench](memory-bench.md) — the operator-path bench: cycle, modes,
  variants, privacy and the private real-ask corpus.
- [KMP and ChronoLoom baseline](performance-baseline.md) — how to reproduce
  read, write and browser measurements.

The `.tsv` files beside these pages are gate inputs, not documents:

| File | Read by |
|:--|:--|
| [`coverage-floors.tsv`](coverage-floors.tsv) | `scripts/ci/rust-coverage.sh`, `scripts/ci/merge-coverage.py` |
| [`bench-floors.tsv`](bench-floors.tsv) | `scripts/ci/memory-bench-quick.sh` (quality-gate workflow) |
| [`retrieval-baseline.tsv`](retrieval-baseline.tsv) | `scripts/ci/retrieval-baseline.sh`, `retrieval_kmp_scorecard`, the memory bench |
| [`retrieval-baseline-shipped-bridge.tsv`](retrieval-baseline-shipped-bridge.tsv) | `scripts/ci/retrieval-baseline.sh` (judged collection with the shipped lexical bridge) |
| [`retrieval-bridge-stress-baseline.tsv`](retrieval-bridge-stress-baseline.tsv) | `scripts/ci/retrieval-baseline.sh`, the memory bench |
| [`jev-baseline.tsv`](jev-baseline.tsv) | `scripts/ci/jev-baseline.sh`, `jev_kmp_scorecard`, the memory bench |
| [`relate-baseline.tsv`](relate-baseline.tsv) | `scripts/ci/relate-baseline.sh`, `relate_kmp_scorecard`, the memory bench |

## Performance records

Dated measurements that [the baseline](performance-baseline.md) cites. Their
numbers hold for the commit and date each one names.

- [Temporal read performance](temporal-read-performance.md)
- [Dimensional read performance](dimensional-read-performance.md)
- [Visual projection performance](visual-projection-performance.md)
- [ChronoLoom scene performance](chronoloom-scene-performance.md)
- [Lexical index performance](lexical-index-performance.md)

## Experimental scripts

Outside the kernel and outside the 1.0 promise.

- [Automatic formation](automatic-formation.md) — source-grounded memory
  writing with a local model, producing a `kmp_ingest` payload.
- [Automatic entities](automatic-entities.md) — cited identity proposals and
  bounded expansion.
