# CLI reference

`kmp-mcp` with no verb serves MCP over stdio: that is what the host runs. The
verbs below are for people. `kmp-mcp <verb> --help` prints the live usage;
this page is that usage, written out. The MCP tools are a separate surface,
listed by the server's `tools/list` and explained by `kmp_guide`.

## Exit codes

| Code | Meaning |
|:--|:--|
| `0` | Done. `info` exits 0 even when its report shows a `FAIL`: it states, `doctor` judges. |
| `1` | The command ran and failed: `doctor` found a `FAIL`; `setup`, `update` or `uninstall` could not finish. |
| `2` | Refused: wrong usage, a refused environment switch, or the command refused to act (an unsupported store, a non-empty store for `import`, a local verb with a remote kernel selected). |

## Diagnose

| Command | What it does |
|:--|:--|
| `kmp-mcp info` | What this binary is and which memory it opens: backend, switches in effect, the data directory and the rule that chose it, store config, tools, viewer, telemetry and every store on this machine. |
| `kmp-mcp doctor` | The binary, backend, switches, memory, store config, database, tools, agent policy, viewer and telemetry, plus host plugins, engines on `PATH` and history, ending with one verdict. Read-only. See [Troubleshooting](../troubleshooting.md). |
| `kmp-mcp --version` | Binary version and store format, as `kmp-mcp <version> (store format 4 (sqlite))`. |

## Install and update

| Command | What it does |
|:--|:--|
| `kmp-mcp setup [--claude] [--codex] [--hermes] [--pi] [--version X.Y.Z] [--engine-dir DIR] [--lexical-bridge FILE \| --no-lexical-bridge] [--dry-run]` | Aligns the installed plugins and engines of the named hosts (all installed hosts when none is named) with one release, and installs the machine's lexical-bridge table. |
| `kmp-mcp update [same flags]` | The same convergence, for every installed host. |
| `kmp-mcp uninstall [--store <absolute-path> \| --engine <absolute-path>] [--apply] [--purge] [--keep-memory]` | Without `--apply`, a preview of everything it would remove. With `--apply`, removes it. A copy of each store is saved in the current directory (`kmp-memory-<name>-<hash>.jsonl`) and verified first, unless `--purge`; `--keep-memory` leaves the stores in place. A committed bundle is never removed. |
| `kmp-mcp plugin resolve-engine --plugin-root DIR` | Used by the plugin launchers: picks the engine that matches both host manifests. |
| `kmp-mcp plugin notice --plugin-root DIR` | Used by the Claude Code session hook: reports version drift. Asks GitHub at most once a day unless `KMP_VERSION_CHECK=off`. |
| `kmp-mcp guide sync --plugin-root DIR [--dry-run]` | Installs the two longer guides (`guide:kmp-agent`, `guide:kmp`) into the selected store. `kmp_guide` needs neither. |

## Configure

| Command | What it does |
|:--|:--|
| `kmp-mcp config` | Shows the agent policy and the memory selection: saved, effective, and the rule that won. |
| `kmp-mcp config memory-routing <on-request\|always>` | `on-request` (default): the agent calls KMP only when asked. `always`: known work enters through `kmp_wake` unprompted. |
| `kmp-mcp config memory-store <absolute-path>` | Saves the store this machine opens when no environment variable or project applies. |
| `kmp-mcp config memory-store --clear` | Back to automatic selection. |

## Memory

| Command | What it does |
|:--|:--|
| `kmp-mcp demo [--no-viewer] [--dry-run]` | Writes the worked example (about `example:kmp-demo`) into the store selected here and opens ChronoLoom on it. Idempotent. It never writes a project's store: inside a project it writes the store a directory outside any project would open (the saved selection, else the per-user default) and says so, so it stays out of the committed bundle and snapshots. `KMP_MCP_DATA_DIR` still decides when set. `--dry-run` only says where it would write. |
| `kmp-mcp memories [--json]` | Every store this machine knows, how it is reached, its format, size and abouts. Opens nothing for use. |
| `kmp-mcp memories register <absolute-path>` | Adds an existing store directory to that list. |
| `kmp-mcp document <about> [--out FILE]` | Renders one about as Markdown. |
| `kmp-mcp summaries pending [<about>…] [--json]` | Lists memories that owe an English search summary (`summary_en`). |
| `kmp-mcp viewer [addr]` | Serves ChronoLoom without an agent session, on a loopback address (default `127.0.0.1:7317`). |

## Back up, restore, move

| Command | What it does |
|:--|:--|
| `kmp-mcp export [file] [--about <about>]...` | Writes a bundle of the whole log or of exact abouts. With no file in a project, writes the committed `.kmp/memory.jsonl`, first catching the store up with a bundle a pull brought ahead. |
| `kmp-mcp export --overwrite-committed` | Replaces a committed bundle whose events the export would not keep (ahead or divergent). |
| `kmp-mcp export --repair-pending` | Publishes and acknowledges a write whose bundle publish was cut short. Stop other sessions first. |
| `kmp-mcp import <file>` | Replays a bundle into an **empty** store. |
| `kmp-mcp import --from <store-dir\|bundle-file> --about <about>...` | Brings exact abouts into the store selected here: imported, unchanged, or extended. A different history refuses the whole import and writes nothing. Running it again is a no-op. |
| `kmp-mcp snapshot create\|list\|verify\|read\|merge ...` | Named, immutable recovery points. Project stores only; elsewhere use `export <file>`. |

Bundles and formats: [Store format](store-format.md).

## Composed contexts and consolidated views

| Command | What it does |
|:--|:--|
| `kmp-mcp context project\|expand [FILE\|-]` | Composes captured memory packets into context JSON, or expands them. Reads no store. |
| `kmp-mcp consolidation sources\|write\|read\|project [FILE\|-]` | Captures, writes, audits or projects versioned consolidated views. |

## Remote kernel

With `KMP_KERNEL_GRPC_ENDPOINT` or `KMP_MCP_BACKEND=grpc` set, `export`,
`import`, `document`, `snapshot`, `summaries`, `memories`, `demo`, `viewer`
and `consolidation` exit 2: they work on the local store only.
