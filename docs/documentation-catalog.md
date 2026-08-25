# Documentation catalog

This catalog describes the maintained documentation surface. It does not
promote archived prose into a current contract.

| Area | Maintained entry point | Executable authority |
|:--|:--|:--|
| Product and interaction | [Root README](../README.md) | `plugins/kmp/capabilities.json`, MCP `tools/list` |
| Local installation and storage | [Embedded KMP](embedded/README.md) | `kmp-mcp --help`, `kmp-mcp info`, `kmp-mcp doctor`, `crates/kmp-mcp` |
| Shared service | [Enterprise KMP](enterprise/README.md) | `Dockerfile`, `distribution/charts/kmp`, `api/proto` |
| Enterprise deployment | [Operations](enterprise/operations.md) | Helm values/templates and deployment scripts |
| Enterprise security | [Security](enterprise/security.md) | transport configuration, HTTP gateway authorization and chart validation |
| Enterprise observability | [Observability](enterprise/observability.md) | `crates/kmp-observability` and chart templates |
| Architecture | [Technical architecture](architecture/README.md) | composition roots, ports, adapters, protocol schemas and tests |
| Operations | [Runbooks](runbooks/README.md) | live CLI help, doctor output, Helm values and CI scripts |
| Tests and releases | [Development](development/README.md) | CI workflows and `scripts/ci` |
| Research | [Research](research/README.md) | experiments and evidence, never release promises |

## Archive boundary

The former root README and entire former `docs/` tree are preserved at
[`archive/docs/audit-2026-08-26`](../archive/docs/audit-2026-08-26/ARCHIVE.md).
They are inputs to an audit, not instructions. Earlier historical material
also remains under [`archive/`](../archive/README.md).

Do not copy an archived command or claim into maintained documentation without
checking it against the current implementation.
