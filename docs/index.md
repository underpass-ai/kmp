# KMP documentation

Read top to bottom the first time; jump to a row after that.

## 1. Start

| Step | Page |
|:--|:--|
| What KMP is, in two minutes | [README](../README.md) |
| Install it in your agent host | [Install](install.md) |
| Write and see a first memory | `kmp-mcp demo`, then [Examples](../examples/README.md) |
| Learn the words | [Concepts](concepts.md) |

## 2. Use

| Need | Page |
|:--|:--|
| Where memory lives, how Ask decides, the viewer, local telemetry | [Embedded KMP](embedded/README.md) |
| Ask across languages | [Languages](embedded/languages.md) |
| The guide agents read: topics, cards, worked examples | [Shipped guides](../plugins/kmp/guide/README.md) |
| The commands for people (`/kmp:*`, `kmp-*`) | [Plugin guide](../plugins/kmp/README.md#what-you-get) |
| ChronoLoom, the viewer | [Viewer](../crates/kmp-viewer/README.md) |

## 3. Reference

| Need | Page |
|:--|:--|
| Every CLI verb, flag and exit code | [CLI reference](embedded/cli.md) |
| Every switch, optional file and config key | [Configuration](embedded/configuration.md) |
| Store and bundle formats, upgrade and rollback | [Store format](embedded/store-format.md) |
| The MCP tools | `tools/list` of the running server, and `kmp_guide {"topic": …}` ([list](concepts.md#the-tools)) |
| What 1.0 promises, what is experimental, support window | [Stability and support](stability.md) |
| What leaves the machine, and when | [Security and privacy](security-privacy.md) |

## 4. Operate

| Need | Page |
|:--|:--|
| Something is wrong | [Troubleshooting](troubleshooting.md) |
| The agent cannot see the tools | [Runbook: MCP tools are missing](runbooks/mcp-tools-missing.md) |
| Recover, move or restore a store | [Runbook: embedded recovery](runbooks/embedded-recovery.md) |
| Upgrade from 0.27 | [Upgrading](upgrading.md) |
| All runbooks | [Runbooks](runbooks/README.md) |

## 5. Shared service (experimental)

| Need | Page |
|:--|:--|
| One memory across machines, agents or services | [Enterprise KMP](enterprise/README.md) |
| Deploy, secure and observe it | [Operations](enterprise/operations.md), [Security](enterprise/security.md), [Observability](enterprise/observability.md) |

## 6. Build KMP

| Need | Page |
|:--|:--|
| How to contribute | [Contributing](../CONTRIBUTING.md) |
| Components, data flows and trust boundaries | [Architecture](architecture/README.md) |
| Tests, releases, the agent surface, retrieval internals | [Development](development/README.md) |
| Questions under study | [Research](research/README.md) |
| Dated design notes and measurements | [Archive](archive/README.md) |
| Every change, release by release | [Changelog](../CHANGELOG.md) |

When prose and the product disagree, the product wins: `tools/list`,
`kmp-mcp --help`, `kmp-mcp doctor`, the API contracts and the CI scripts are
the source of truth. [Which source owns what](documentation-catalog.md).
