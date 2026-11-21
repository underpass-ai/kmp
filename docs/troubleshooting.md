# Troubleshooting

Start with one command:

```bash
kmp-mcp doctor
```

Run it from the directory the host works in, with the same environment.
It ends with one verdict:

| Last line | Exit | Meaning |
|:--|:--|:--|
| `Usable.` | 0 | Nothing to fix. |
| `Usable, with a warning above.` | 0 | It works; read the `warn` lines. |
| `Not usable. Fix the FAIL above first.` | 1 | Fix the first `FAIL` line, then run it again. |

`kmp-mcp doctor` opens the store read-only and changes nothing. Every quote
below is real output of the 1.0 engine (`<dir>` stands for your path).

## The tools are missing in the agent

The host shows no `kmp_*` tools. Follow the
[missing-tools runbook](runbooks/mcp-tools-missing.md). The usual causes:

| Cause | Fix |
|:--|:--|
| The session started before setup or an update | Restart the host. A running session keeps the tool list it started with. |
| No engine is installed | `/kmp:setup` (Claude Code), `kmp-setup` (Codex), or `kmp-mcp setup`. |
| The server refused to start (see the next sections) | Fix the `FAIL` that `doctor` names. |

## Environment

| You see | Cause | Fix |
|:--|:--|:--|
| `FAIL  KMP_LEXICAL_INDEX holds a value kmp-mcp refuses` / ``KMP_LEXICAL_INDEX=`maybe` is not a value kmp-mcp accepts; it accepts on, off, true, false, 1, 0, yes, no`` | A switch value this build does not accept. The server does not start; other verbs exit 2. | Fix the value or unset the variable. Accepted values: [configuration](embedded/configuration.md#environment-switches). |
| ``KMP_MCP_DATA_DIR=`relative/path` is not a value kmp-mcp accepts; it accepts an absolute path (no ~, not relative).`` | Relative store path. | Use an absolute path, or unset it. |
| ``viewer address `0.0.0.0:7317` is not a loopback address`` | The viewer serves private memory and binds loopback only. | A loopback address, `off`, or unset `KMP_VIEWER_ADDR`. |
| ``FAIL  `fixture` is not a backend`` | The test backend is not in a release binary. | Unset `KMP_MCP_BACKEND`. |
| `warn  KMP_MCP_ENGINE is set, and this build reads no such switch` | A typo, a switch removed at 1.0, or another program's variable. | Unset it. |
| `kmp-mcp export: this command works on the local store, and the environment selects a remote gRPC kernel …` | `KMP_MCP_BACKEND=grpc` or `KMP_KERNEL_GRPC_ENDPOINT` is set. | Unset both to work on this machine's store. |

## The store

| You see | Cause | Fix |
|:--|:--|:--|
| ``embedded store at `<dir>` uses unsupported format version 1; current KMP opens format 4 only and left the directory untouched`` (or version 2) | A store from 0.14 or earlier. | Select another store (`kmp-mcp config memory-store --clear`, or move the directory aside). No migration exists; see [Store format](embedded/store-format.md). |
| `uses format version 9, newer than this binary supports (4); upgrade the binary` | A newer engine wrote it. | `kmp-mcp update`. |
| `FAIL  the selected memory's database cannot be read` (section **Database**) with `database disk image is malformed` or `file is not a database` | `store/kernel.sqlite3` is damaged. | Move the data directory aside, then `kmp-mcp import <bundle>` into the new empty store from your last export. In a project, the next session rebuilds the store from `.kmp/memory.jsonl`. |
| ``lost its database: `<dir>/store/kernel.sqlite3` is missing, but this directory shows it held memory before`` | The database was deleted or emptied. KMP refuses to start an empty store over it. | Restore the file from a backup. In a project, move the directory aside: the next session rebuilds it from the committed bundle. |
| `warn  the committed memory is ahead of the live store` | A `git pull` brought a teammate's events. | Nothing: the next session catches up on open. |
| `FAIL  the live store and committed memory are divergent histories` | The store and `.kmp/memory.jsonl` forked. Reads answer from the store with a notice; writes are refused. | Keep the committed history: `mv .kernel .kernel.mine` and reopen; bring back an about only you wrote with `kmp-mcp import --from .kernel.mine --about <about>`, then `kmp-mcp export`. Keep yours instead: `kmp-mcp export --overwrite-committed` and commit. |
| `FAIL  1 write is not proved in the committed bundle` | A write committed but its bundle publish was cut short. | Usually the next session publishes it. Otherwise stop other KMP sessions, `kmp-mcp export`, check the diff, then `kmp-mcp export --repair-pending` and commit. |
| ``the saved user memory selection is unusable: …`` (`config` exits 2) | `config.toml` holds an invalid `memory_store`. | `kmp-mcp config memory-store --clear`. |
| ``cannot select this memory: `<dir>` holds memory this engine cannot open`` | `config memory-store` refused a directory it cannot open; nothing in it changed. | Choose another directory. |

## Commands that refuse

| You see | Cause | Fix |
|:--|:--|:--|
| ``import refused; nothing was written. `kmp-mcp import <file>` restores a bundle whole into an empty store, and this store already holds 1 event`` | `import <file>` restores into an empty store only. | The refusal names both ways: to add exact abouts to a store that holds memory, `kmp-mcp import --from <bundle\|store-dir> --about <about>`; to restore the whole bundle, point `KMP_MCP_DATA_DIR` at a new directory. |
| ``import refused; nothing was written. The source `<dir>` holds no about `project:nope` `` (or `the source holds no about` for a bundle) | `import --from` asked for an about the source does not have. | Check the about with `kmp-mcp memories` or the bundle header. |
| `named snapshots belong to a project, but this store resolved to <dir> by the user rule` | Snapshots exist only for project stores. | `kmp-mcp export <file>`. |
| `export needs a bundle file path here` | Outside a project there is no committed bundle to write. | `kmp-mcp export <file>`. |

## Tools that refuse

| Code or text | Cause | Fix |
|:--|:--|:--|
| `invalid_argument` ending in `Help: kmp_guide {"topic":"…"}` | The call breaks the schema or a named rule. | Read that topic; the refusal names the field and the fix. |
| `GUIDE_UNAVAILABLE` (from `kmp_inspect` of a guide node) | The longer stored guide is not installed in this store. A read never installs it. | `kmp-mcp guide sync --plugin-root <plugin dir>` (or `/kmp:guide`), then retry. `kmp_guide {topic}` works without it. |
| `CONTINUATION_UNAVAILABLE` | The handle expired (24 hours), or came from 0.27. | Repeat the original call. |
| `CONTINUATION_ARGUMENT_DIFFERS` | A continuation was sent with a different argument value. | Send the handle alone. |
| `retryable_conflict` | Another writer won every retry. Nothing was applied. | Replay with the same idempotency key. |
| `conflict` | A key reused with different content, a moved view, or a guard that needs a decision. | Do not replay unchanged; follow the message. |

## The viewer

| You see | Cause | Fix |
|:--|:--|:--|
| A link on another port than 7317 | Another session holds 7317. | Nothing to do. |
| `kmp_view_open` with `"viewer_available":false` and ``ChronoLoom could not start: viewer could not bind `127.0.0.1:7317`: Address already in use`` | You pinned `KMP_VIEWER_ADDR` to a busy port. | Another port, or unset it. |
| `401` and `this ChronoLoom session needs its capability link` | An old or copied link, or another program. | Ask the agent to open the view again for a new link. |
| `KMP_VIEWER_ADDR turns the viewer off` | `KMP_VIEWER_ADDR=off` with `kmp-mcp viewer`. | Unset it or pass an address. |

## Store configuration files

`doctor` and `info` read each optional file beside the store as a new
process would:

| You see | Cause | Fix |
|:--|:--|:--|
| ``warn  ask-gate.json: rejected: ask-gate.json names mode `bogus` `` | A value the file does not accept. The default applies. | Fix or remove it, then restart the host. |
| `warn  lexical-bridge.kmpb: rejected: the table is unreadable or empty` | Damaged bridge table. Asks still match within one language. | Remove it, or `kmp-mcp setup` to install the machine table. |
| `warn  typesafe.json: rejected: TYPESAFE_API_KEY is not set and no ~/.config/typesafe.env holds it` | Jev opted in without a key. Nothing is sent. | Set `TYPESAFE_API_KEY`, or write `typesafe.env` in your config directory (mode `0600`). |
| `warn  typesafe.json: rejected: TypeSafe model must be a pinned version such as jev-1.14.0` | An alias such as `jev-latest`. | Name a pinned release. |
| `warn  ask-judge.json: rejected: ask-judge.json needs typesafe.json beside the store` | A Jev feature without its parent opt-in. | Add `typesafe.json`, or remove the file. |

## Hosts and engines

| You see | Cause | Fix |
|:--|:--|:--|
| `warn  no kmp-mcp is on PATH` | A Claude Code-only install runs the plugin's own engine; nothing else would find one. | Add the install directory (`~/.local/bin` by default) to `PATH`, or `kmp-mcp setup`. |
| `FAIL  codex: effective engine failed its runtime proof` / `… did not report <version>` | An older engine comes first on `PATH`. | Put the current engine first, or `kmp-mcp setup`; remove an old one with `kmp-mcp uninstall --engine <path>`. |
| `warn  codex: MCP registration installed and enabled, live connection unverified` | `doctor` cannot open a host session itself. | Nothing to fix; ask the agent to call a `kmp_*` tool to confirm. |

One plugin owns the MCP registration. Do not also add `[mcp_servers.kmp]`
to Codex's `config.toml` (`$CODEX_HOME`, default `~/.codex`) or
`claude mcp add kmp` by hand. `doctor` reads the same files as the
`kmp-mcp uninstall` preview and reports such a second owner beside an
installed plugin, and a retired `kernel-memory` registration anywhere:

| You see | Cause | Fix |
|:--|:--|:--|
| `warn  codex: a second MCP owner is wired by hand beside the plugin` / `delete the [mcp_servers.kmp] block, then restart codex` | A hand-written `[mcp_servers.kmp]` table beside the plugin. | Delete the block from the file it names, then restart the host. |
| ``warn  claude: a retired `kernel-memory` registration is still wired`` / `remove with: claude mcp remove kernel-memory, then restart claude` | Wiring from before the server was named `kmp`. | Run the command it names, then restart the host. |
