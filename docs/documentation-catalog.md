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

## History boundary

Superseded documents remain available in Git history and release tags; they
are evidence, not current instructions. Check any historical command or claim
against the current implementation before reusing it.
