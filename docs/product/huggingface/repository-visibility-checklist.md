# Repository Visibility Checklist

Status: working checklist for the first public operator-model release.

This checklist is for making the kernel repository clear to readers arriving
from Hugging Face, Dev.to, LinkedIn, GitHub search, or a model card.

The goal is credibility, not hype. Every visible claim should point to code,
documentation, or a reproducible result.

## First Screen

The first screen of the repository should answer four questions:

| Question | Desired answer |
| --- | --- |
| What is this? | KMP is the Kernel Memory Protocol for navigable agent memory. |
| Why does it matter? | Agents can query, traverse, inspect, and audit memory instead of only retrieving chunks. |
| How do I try it? | Quickstart with one write/read/trace/inspect flow. |
| What is proven? | Benchmarks and live replay results, separated by methodology. |

Avoid starting with storage backend names. Backend choices are implementation
details. The public concept is KMP memory: about scopes, dimensions, temporal
movement, relations, evidence, and inspection.

## README Changes Before Public Model Release

- Rename the top-level framing from generic "context rehydration" to
  KMP / Kernel Memory Protocol.
- Show one compact KMP lifecycle:
  `ingest -> near -> trace -> inspect`.
- Include an architecture diagram focused on KMP boundaries:
  API clients, MCP adapter, kernel, graph persistence, key-value persistence,
  event persistence, observability.
- Keep concrete current adapters visible, but introduce them as adapters:
  Neo4j for graph persistence, Valkey for key-value persistence, NATS for event
  persistence/streaming.
- Add a short "What this is not" section:
  not a final answer model, not a benchmark solver, not hidden agent state, not
  a vector database replacement.
- Add benchmark/result tables with labels:
  official benchmark, local scorecard, reader check, live replay.
- Link the Hugging Face model and dataset only after the publication gate is
  green.
- Link the Dev.to article as product background, not as proof.
- Keep license and author visible.

## GitHub Metadata

Recommended repository topics:

- `ai-agents`
- `agent-memory`
- `mcp`
- `grpc`
- `knowledge-graph`
- `temporal-memory`
- `rust`
- `llm-tools`
- `event-sourcing`
- `opentelemetry`

Recommended short description:

```text
Kernel Memory Protocol for navigable, temporal, multidimensional AI agent memory.
```

## Release Assets

Before making the Hugging Face repos public, prepare:

- GitHub release with kernel commit, model tag, dataset tag, and eval summary;
- model card copied from
  `docs/product/huggingface/kernel-tool-operator-small-model-card-template.md`;
- dataset card copied from
  `docs/product/huggingface/kernel-operator-trajectories-dataset-card-template.md`;
- evaluation summary copied from
  `docs/product/huggingface/operator-release-eval-summary-template.md`;
- direct links to reproducible commands in `scripts/operator/README.md`;
- a short LinkedIn/Dev.to update that says exactly what was released and what
  it does not claim.

## Contributor-Friendly Issues

Open small issues after release so new readers have a path in:

- improve KMP quickstart examples;
- add a minimal local demo with embedded adapters when available;
- add a conformance test for a storage adapter;
- add a visual replay of memory traversal;
- add a new interpretation plugin outside kernel core;
- improve operator trajectory visualization.

## Red Flags

Do not publish if the repo front page:

- overclaims benchmark results;
- mixes internal validation with final public release results;
- hides the fact that the operator is a tool-use specialist;
- suggests the model answers questions directly;
- presents backend storage as the product;
- leaves users without a runnable or inspectable path.
