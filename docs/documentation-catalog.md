# Documentation catalog

Date: 2026-08-25

This catalog separates the small current documentation surface from retained
history. If prose conflicts with the executable implementation, the live CLI,
MCP schemas, protobuf contracts, chart validation and tests win.

## Current surface

| Area | Authority |
|:--|:--|
| Product entry point and user interaction | [README](../README.md) |
| Local and deployed topology | [Editions](editions.md) |
| Embedded operation | [Embedded operations](operations/embedded/README.md) and [`crates/kmp-mcp`](../crates/kmp-mcp/README.md) |
| Docker and Kubernetes operation | [Deployment operations](operations/deployment/README.md), [`Dockerfile`](../Dockerfile) and [`distribution/charts/kmp`](../distribution/charts/kmp) |
| Plugin, skills and MCP ownership | [`plugins/kmp`](../plugins/kmp/README.md) and [`capabilities.json`](../plugins/kmp/capabilities.json) |
| Typed service contracts | [`api/proto`](../api/proto/README.md) and [`api/asyncapi`](../api/asyncapi/) |
| Guarantees and limitations | [Runtime guarantees](runtime-guarantees.md) and [security model](security-model.md) |
| Verification | [Testing](testing.md), [runtime guarantees](runtime-guarantees.md) and the repository CI scripts |
| Release artifacts | [Release process](release.md) and the release workflows |

The exhaustive classification of active `docs/**/*.md` files is in
[`documentation-inventory.tsv`](documentation-inventory.tsv). CI checks the
current class against the live KMP vocabulary and keeps it reachable from
[`index.md`](index.md).

## Research

The [research summary](research/README.md) owns the active investigation into
temporal and multidimensional memory analysis, decision outcomes, recurring
conversation patterns and state-of-the-art agentic context. Research may guide
future work, but it is not a release promise.

## Archive

The following collections are deliberately outside the current documentation
spine:

| Collection | Why retained |
|:--|:--|
| [Architecture decisions](../archive/docs/adr/README.md) | Historical rationale and experimental evidence. |
| [Product plans](../archive/docs/product/README.md) | Former roadmaps, API plans, Operator work and publication drafts. |
| [Operations guides](../archive/docs/operations/README.md) | Superseded host, release, storage, Docker, Kubernetes and TLS procedures. |
| [Runbooks](../archive/docs/runbooks/README.md) | Procedures that no longer represent a maintained operational surface. |
| [Migrations](../archive/docs/migration/README.md) | Compatibility and integration history. |
| [Integrations](../archive/docs/integrations/made-kmp.md) | Former product-specific guidance. |
| [Research archive](../archive/docs/research/README.md) | Benchmarks, papers, demos, incidents and completed notebooks. |
| [Showcase](../archive/docs/showcase/README.md) | Former public-pitch recordings and sources. |

Archived paths may still appear inside fixtures or recorded evidence because
those strings describe what existed when the evidence was produced. New code
and current documentation must not treat those paths as live instructions.

## Rules

- Do not call a plan implemented unless a code path and verification exist.
- Do not copy commands from the archive without revalidating them against the
  current binary or deployment artifact.
- Keep normal local use first; keep Docker and Kubernetes explicitly optional.
- Keep plugin workflows, skills, MCP tools and kernel behavior as separate
  layers with one declared owner each.
- Keep known limitations next to the current user-facing contract.
