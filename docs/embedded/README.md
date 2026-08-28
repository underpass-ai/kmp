# Embedded KMP

Embedded KMP is the default product path. The coding-agent host starts
`kmp-mcp` as a local child process, communicates over stdio and runs the kernel
inside that process. No KMP server, database, account or API key is required.

## Install

Use the native KMP plugin where available:

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

For Codex, ask the agent to run `kmp-setup`. Restart the host once after a
setup or plugin update, because a running session keeps the MCP inventory it
started with.

The plugin must be the single MCP owner. Do not combine it with standalone
global `mcp_servers.kmp` or retired `mcp_servers.kernel-memory` wiring.

Standalone fallback:

```bash
cargo install kmp-mcp --locked
```

Then register the resulting binary once with the host. Detailed plugin
ownership and packaging live in [`plugins/kmp`](../../plugins/kmp/README.md).

## Verify

```bash
kmp-mcp info
kmp-mcp doctor
```

`info` identifies the binary, selected backend, data directory, engine,
durability state and viewer. `doctor` diagnoses the process-local setup. The
plugin's `kmp-doctor` workflow also checks host wiring and duplicate ownership.

`KMP_MCP_BACKEND=fixture` is only for deterministic protocol tests. It stores
nothing and must never be mistaken for a real memory.

## Where memory lives

The data directory is selected in this order:

1. `KMP_MCP_DATA_DIR`, when explicitly set;
2. `.kernel/` at the nearest git root;
3. the per-user local data directory: `$XDG_DATA_HOME/kmp/default` or
   `~/.local/share/kmp/default` on Unix, and `%LOCALAPPDATA%\kmp\default`
   on Windows (with `APPDATA` and `USERPROFILE` fallbacks).

The current shipped `kmp-mcp` binary creates fresh stores on SQLite. Existing
redb stores are supported as a legacy migration path: KMP opens the engine
stamped on disk and never silently substitutes an empty SQLite store.

Do not create a new redb store. To change the store used by a project, point
`KMP_MCP_DATA_DIR` at the intended directory and verify the selection with
`kmp-mcp info` before writing.

SQLite permits multiple local agent hosts to share one store. `kmp-mcp
share-memory` snapshots, migrates and verifies a legacy redb store before
replacing it with SQLite, while keeping the original.

## Durability and recovery

- writes are committed durably before success is returned;
- embedded projection is synchronous, so successful writes are immediately
  readable;
- project stores maintain `.kmp/memory.jsonl` as a portable event bundle;
- import only restores into an empty store;
- named snapshots are immutable recovery points;
- format and engine mismatches fail instead of opening an empty neighboring
  store.

The store itself is machine state and is ignored by git. The portable bundle
can be reviewed and committed deliberately; it may contain the evidence that
was written, so treat it with the same confidentiality as the store.

## Viewer and local telemetry

An embedded session attempts to serve a read-only viewer rooted at
`http://127.0.0.1:7317/`. It binds loopback only and prints a random,
per-session capability link. `kmp_view_open` and `kmp_view_get_state` return
the same link, so an agent can hand it over even when its host hides server
output. Opening the link exchanges the capability for an HttpOnly, SameSite
cookie and redirects to the clean URL; requests from other local processes
receive `401`. Set `KMP_VIEWER_ADDR` to a different loopback address or `off`
to disable it.

Logs and the bounded quality journal live inside the data directory. They are
local diagnostics, not remote telemetry. KMP does not upload memory to
Underpass.

## Maintenance commands

Run `kmp-mcp --help` for the live command contract.

| Command | Purpose |
|:--|:--|
| `info`, `doctor`, `config` | Identify the installation, diagnose it and configure Ask fallback languages. |
| `export`, `import` | Checkpoint or restore an event bundle. |
| `snapshot create|list|verify|read|merge` | Create and inspect immutable recovery points. |
| `document <about>` | Render one about as deterministic Markdown. |
| `migrate <src> <dst> [--engine ...]` | Replay memory into a fresh store. |
| `share-memory` | Convert a legacy redb store to verified SQLite. |
| `viewer [addr]` | Serve the viewer without an MCP host session. |
| `uninstall [--apply]` | Preview removal, then apply only when explicitly requested. |

The ten MCP memory tools are a separate surface advertised by `tools/list`.

## Limits

Embedded KMP shares memory between local processes that can access the same
directory. It is not a network service and does not provide remote identity,
authorization, high availability or centralized operations. Use
[Enterprise KMP](../enterprise/README.md) only when those are actual
requirements.

## Implementation authority

- [`crates/kmp-mcp`](../../crates/kmp-mcp/)
- [`crates/kmp-embedded`](../../crates/kmp-embedded/)
- [`crates/kmp-adapter-embedded`](../../crates/kmp-adapter-embedded/)
- [`crates/kmp-viewer`](../../crates/kmp-viewer/)
- [`plugins/kmp/capabilities.json`](../../plugins/kmp/capabilities.json)
