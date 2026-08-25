# Embedded operations

Embedded KMP is the default and recommended topology. The coding-agent host
starts `kmp-mcp` as a local child process, speaks MCP over stdio, and runs the
kernel inside that same process. There is no KMP account, hosted memory
service, external database or memory network connection.

## Install and verify

Install the KMP plugin for Codex or Claude Code, run its `kmp-setup` workflow,
and restart the host once. The complete current path is the repository
[quickstart](../../../README.md#your-first-local-memory-in-two-minutes).

```bash
# Codex CLI
codex plugin marketplace add underpass-ai/plugins
codex plugin add kmp@underpass
```

```text
# Claude Code
/plugin marketplace add underpass-ai/plugins
/plugin install kmp@underpass
/kmp:setup
```

Then use:

```bash
kmp-mcp info
kmp-mcp doctor
```

`info` identifies the selected store, engine, durability state and viewer.
`doctor` is the diagnostic path when the binary, MCP ownership, store or tool
surface is wrong. The plugin-level doctor additionally checks host wiring.

The plugin is the single MCP owner. Do not add a second global
`mcp_servers.kmp` or legacy `mcp_servers.kernel-memory` registration when the
plugin is enabled. The supported plugin and standalone paths are documented in
[`plugins/kmp/README.md`](../../../plugins/kmp/README.md).

## Local data

The selected data directory follows this order:

1. explicit `KMP_MCP_DATA_DIR`;
2. `<git-root>/.kernel/`;
3. `$XDG_DATA_HOME/kmp/default`, or `~/.local/share/kmp/default`.

Fresh stores use SQLite and can be opened by concurrent local hosts. Existing
redb stores remain supported and single-writer; `kmp-mcp share-memory` creates
and verifies a SQLite replacement while preserving the original.

The store is machine state. For a project-scoped store, KMP maintains the
portable event bundle at `.kmp/memory.jsonl`; it can be reviewed or committed
deliberately. The read-only viewer is offered on
`http://127.0.0.1:7317/`. It binds loopback only and can be moved or disabled
with `KMP_VIEWER_ADDR`.

KMP does not send the store to Underpass. Installation and update checks may
contact GitHub for release metadata and checksummed binaries; that is package
delivery, not memory synchronization.

## Maintenance surface

Run `kmp-mcp --help` for the live command contract. The current maintenance
surface includes:

| Command | Purpose |
|:--|:--|
| `info`, `doctor`, `config` | Identify the installation, diagnose it and configure agent routing policy. |
| `export`, `import` | Checkpoint or restore the portable event bundle. Import requires an empty store. |
| `snapshot create|list|verify|read|merge` | Manage immutable local recovery points and safe historical reads. |
| `document <about>` | Render one about as deterministic Markdown. |
| `migrate <src> <dst>` | Replay a store into a new directory or engine. |
| `share-memory` | Convert an existing redb store to shareable SQLite safely. |
| `viewer [addr]` | Serve the local read-only viewer without an agent session. |
| `uninstall` | Preview removal; mutation requires the explicit apply path. |

The MCP memory surface is separate from these maintenance commands. Its ten
tools are advertised by the running server through `tools/list`; the plugin
skills decide when and how to compose them.

## Authority

When prose and implementation disagree, prefer these current sources:

- CLI and backend behavior: [`crates/kmp-mcp`](../../../crates/kmp-mcp/README.md);
- plugin installation and ownership: [`plugins/kmp`](../../../plugins/kmp/README.md);
- current release artifacts and versions: [release process](../../release.md).
