# Documentation catalog

Which document describes each area, and which executable source wins when
they disagree.

| Area | Document | Executable authority |
|:--|:--|:--|
| Product and interaction | [Root README](../README.md), [Concepts](concepts.md) | `plugins/kmp/capabilities.json`, MCP `tools/list` |
| Installation | [Install](install.md) | `kmp-mcp setup --help`, `kmp-mcp doctor` |
| Local use and storage | [Embedded KMP](embedded/README.md), [Store format](embedded/store-format.md) | `kmp-mcp --help`, `kmp-mcp info`, `kmp-mcp doctor`, `crates/kmp-mcp` |
| CLI | [CLI reference](embedded/cli.md) | `kmp-mcp <verb> --help` |
| Configuration | [Configuration](embedded/configuration.md) | `crates/kmp-mcp/src/environment/switch_catalogue.rs`, `kmp-mcp doctor` |
| Compatibility and support | [Stability and support](stability.md), [Upgrading](upgrading.md) | the changelog |
| Security and privacy | [Security and privacy](security-privacy.md), [SECURITY.md](../SECURITY.md) | the code each row of that page names |
| Diagnosis and repair | [Troubleshooting](troubleshooting.md), [Runbooks](runbooks/README.md) | `kmp-mcp doctor` |
| Agent guidance | [Shipped guides](../plugins/kmp/guide/README.md) | `kmp_guide`, `plugins/kmp/guide/editorial.json` |
| Shared service (experimental) | [Enterprise KMP](enterprise/README.md) | `Dockerfile`, `distribution/charts/kmp`, `api/proto` |
| Architecture | [Technical architecture](architecture/README.md) | composition roots, ports, adapters, protocol schemas and tests |
| Tests and releases | [Development](development/README.md) | CI workflows and `scripts/ci` |
| Research | [Research](research/README.md) | experiments and evidence, never release promises |

## History

Dated records of the work before 1.0 are in the [archive](archive/README.md).
The documentation tree before 2026-08-26 is in the
[`v0.5.0` Git tree](https://github.com/underpass-ai/kmp/tree/v0.5.0/archive).
Both are evidence, not instructions: check any historical command or claim
against the current implementation before reusing it.
