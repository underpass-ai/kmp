# Memory Viewer (embedded edition)

A local, read-only web UI over KMP memory — the graph with its typed
relations, the note behind each node, known-at-time timeline movement, and
causal traces. Decision record: [ADR-017](../adr/ADR-017-embedded-memory-viewer.md).

The viewer serves **private memory**; it binds loopback addresses only and
refuses non-local `Host` headers. There is no authentication — do not port-forward
it beyond the machine you trust with the store itself.

## Watching a live agent session

The embedded store is single-writer
([ADR-011](../adr/ADR-011-embedded-concurrency-model.md)), so a live session
can only be observed from inside that session's process. Add one variable to
the MCP server config:

```json
{
  "mcpServers": {
    "kernel-memory": {
      "command": "rehydration-mcp",
      "env": {
        "REHYDRATION_MCP_BACKEND": "embedded",
        "REHYDRATION_VIEWER_ADDR": "127.0.0.1:7317"
      }
    }
  }
}
```

The session logs `memory viewer at http://127.0.0.1:7317/` to stderr on
startup. Every write the agent makes projects synchronously, so a reload of
the viewer sees it immediately.

## Standalone (no session running)

```bash
REHYDRATION_MCP_DATA_DIR=~/.local/share/kernel rehydration-mcp viewer          # 127.0.0.1:7317
REHYDRATION_MCP_DATA_DIR=~/.local/share/kernel rehydration-mcp viewer 127.0.0.1:9000
```

Data-dir resolution is the same as `export`/`import`
([ADR-012](../adr/ADR-012-embedded-data-directory.md)). If an agent session
already holds the store, the command fails fast with the single-writer error —
use the in-session mount above instead.

## What maps to what

| Viewer | Kernel read | Notes |
|:-------|:------------|:------|
| Sidebar abouts | about index | same list `kernel_wake` scopes against |
| Graph tab | `wake` | depth/budget/scope controls mirror the tool arguments |
| Node panel | `inspect` | detail text, properties, raw coordinates, typed links both ways with `why`/`evidence` quoted verbatim |
| Timeline tab | `temporal` | `known at` = `goto` with a time cursor; `near`/`rewind`/`forward` with the selected node as `ref` cursor |
| Trace tab | `trace` | highlights the proven path on the graph; shows the rendered path context and its hash |
| Context button | the `wake` rendering | the exact text a model would receive, with the hash that covers it |
| Status bar | bundle metadata + quality | revision, snapshot hash, tokens, compression, causal density, noise |

Double-click a node to expand its neighborhood; click a legend kind to dim
it; search matches titles, summaries and ids.

## HTTP surface

GET only, JSON errors, loopback only: `/api/info`, `/api/abouts`,
`/api/graph`, `/api/node`, `/api/nodes`, `/api/timeline`, `/api/trace`. The
UI and its render engine are compiled into the binary; nothing is fetched at
runtime. Vendored-asset provenance and the supply-chain verification record
live in
[`crates/rehydration-viewer/ui/vendor/VENDOR.md`](../../crates/rehydration-viewer/ui/vendor/VENDOR.md).
