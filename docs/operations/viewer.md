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
can only be observed from inside that session's process. That mount is
automatic: **every embedded session serves the viewer at `127.0.0.1:7317`**
without being asked. There is nothing to add to the MCP config.

```json
{
  "mcpServers": {
    "kernel-memory": {
      "command": "kmp-mcp",
      "env": { "KMP_MCP_BACKEND": "embedded" }
    }
  }
}
```

The session logs `memory viewer at http://127.0.0.1:7317/` to stderr on
startup, and hands the same link back on the first memory it writes — the
first moment there is anything to look at. `kmp-mcp info`, `kmp-mcp doctor`
and `/kmp:doctor` all print the address, the last of them by asking the port
rather than trusting the configuration. Every write the agent makes projects
synchronously, so a reload of the viewer sees it immediately.

### Choosing another address, or none

| `KMP_VIEWER_ADDR` | What happens |
|:--|:--|
| unset | the viewer serves at `127.0.0.1:7317` |
| `127.0.0.1:9000` | it serves there instead, and the session **fails** if it cannot bind |
| `off`, `none`, empty | no viewer this session |

The asymmetry is deliberate. An address you named must be honoured or refused
out loud, because a typo that silently serves nothing wastes an afternoon. An
address the binary offered on your behalf must never cost you your memory: if
another project's session already holds the port, this one warns on stderr and
carries on without a viewer. That session's viewer still works — open it.

Two projects open at once, each wanting its own viewer, is the case for naming
the second one:

```json
"env": { "KMP_MCP_BACKEND": "embedded", "KMP_VIEWER_ADDR": "127.0.0.1:7318" }
```

## Standalone (no session running)

```bash
KMP_MCP_DATA_DIR=~/.local/share/kernel kmp-mcp viewer          # 127.0.0.1:7317
KMP_MCP_DATA_DIR=~/.local/share/kernel kmp-mcp viewer 127.0.0.1:9000
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
[`crates/kmp-viewer/ui/vendor/VENDOR.md`](../../crates/kmp-viewer/ui/vendor/VENDOR.md).
