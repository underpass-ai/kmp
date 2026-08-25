# Operations

KMP has two operational topologies. Start with the embedded topology unless
you need one centrally operated memory shared by several people, agents or
services.

| Topology | Use it when | Start here |
|:--|:--|:--|
| Embedded | One workstation or repository needs local, private memory with no service to operate. This is the default. | [Embedded operations](embedded/README.md) |
| Deployed | Several clients need one live service operated with Docker or Kubernetes. | [Docker and Kubernetes operations](deployment/README.md) |

Both use the same KMP memory model and ten MCP tools. The difference is where
the kernel and its storage run.

The former detailed runbooks were archived after an August 2026 review found
that they mixed superseded release, storage and cluster assumptions. They are
available as historical evidence under
[`archive/docs/operations`](../../archive/docs/operations/README.md), but they
are not an operational contract.

Related current documentation:

- [edition comparison](../editions.md);
- [enterprise topology](../enterprise.md);
- [security model](../security-model.md);
- [release process](../release.md);
- [testing](../testing.md) and [observability](../observability.md).
