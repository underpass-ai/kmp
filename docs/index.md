# Documentation index

KMP is local and private by default. Start with the repository
[README](../README.md); come here when you need a specific contract or
operational path.

## Use and operate KMP

| Need | Current document |
|:--|:--|
| Install locally and understand normal interaction | [README](../README.md) |
| Compare local and deployed topologies | [Editions](editions.md) |
| Operate local KMP | [Embedded operations](operations/embedded/README.md) |
| Operate the image or Helm chart | [Docker and Kubernetes operations](operations/deployment/README.md) |
| Understand the shared Kubernetes topology | [Enterprise KMP](enterprise.md) |
| Configure security boundaries | [Security model](security-model.md) |
| Read runtime guarantees | [Runtime guarantees](runtime-guarantees.md) |

The operations router is [`operations/README.md`](operations/README.md).

## Integrate and develop

| Need | Current document |
|:--|:--|
| Use the typed APIs and model-producer path | [Usage guide](usage-guide.md) |
| Start with GraphBatch | [GraphBatch quickstart](graph-batch-quickstart.md) |
| Inspect the experimental GraphBatch boundary | [GraphBatch ingestion API](graph-batch-ingestion-api.md) |
| Run tests and quality gates | [Testing](testing.md) |
| Run the live E2E preflight | [Running E2E](development/running-e2e.md) |
| Inspect telemetry | [Observability](observability.md) |
| Cut and verify a release | [Release process](release.md) |

## Research and history

- [Research summary](research/README.md) describes completed work and the
  current state-of-the-art agentic-context investigation.
- [Documentation catalog](documentation-catalog.md) explains what is current,
  research or archived.
- [Archive](../archive/README.md) preserves former product plans, ADRs,
  detailed operations guides, runbooks, migrations, integrations, experiments
  and artifacts. Archived prose is evidence, not the current contract.
