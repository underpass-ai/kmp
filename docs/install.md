# Install

KMP is one binary, `kmp-mcp`, that your agent host starts as a local MCP
server. The plugin of each host owns that registration: install the plugin,
run setup, restart the host once. Do not also add a KMP server by hand.

## Hosts

| Host | Install | Who registers the MCP server | Skills |
|:--|:--|:--|:--|
| Claude Code | `/plugin marketplace add underpass-ai/kmp@marketplace`, `/plugin install kmp@underpass`, then `/kmp:setup` | The plugin (`.mcp.json` → launcher script) | From the plugin, as `/kmp:*` commands |
| Codex CLI | `codex plugin marketplace add underpass-ai/kmp --ref marketplace`, `codex plugin add kmp@underpass`, then ask Codex to run `kmp-setup` | The plugin (runs `kmp-mcp` from `PATH`) | From the plugin, as `kmp-*` skills |
| Hermes Agent | Install the Claude Code or Codex plugin first (it carries the skills), then `kmp-mcp setup --hermes` | `setup` runs `hermes mcp add kmp` | Copied into `$HERMES_HOME/skills` by `setup` |
| Pi coding agent | `underpass setup` installs the `pi-runtime` package that carries KMP into Pi; then `kmp-mcp setup --pi` | The `pi-runtime` package (runs `kmp-mcp` from `PATH`) | Mirrored only when `kmp-mcp` runs from a source checkout; a released binary only checks the registration |
| MCPB hosts (Claude Desktop and others) | Open the `.mcpb` package attached to each GitHub Release | The MCPB package | None |

Restart the host after setup or an update: a running session keeps the tool
list it started with.

Details per host, including release tarballs and the launcher scripts:
[plugin guide](../plugins/kmp/README.md#install).

## Platforms

| Platform | Engine for `setup` / `update` | Plugin package |
|:--|:--|:--|
| Linux x86_64 | Yes | Yes |
| Linux arm64 | Yes | Yes |
| macOS arm64 | Yes | Yes |
| macOS x86_64 | Yes | No (engine only) |
| Windows x86_64 | **No.** `setup` and `update` say to use `cargo install`. | Yes |

Release builds target glibc on Linux. On any other platform, or when no
engine asset fits, build the engine from crates.io (Rust 1.97 or newer;
`protoc` must be on `PATH`):

```bash
cargo install kmp-mcp --locked
```

Then keep the plugin as the MCP owner and rerun its setup.

The Windows launcher `plugins/kmp/scripts/run-embedded-mcp.cmd` exists, but
the plugin's `.mcp.json` names only the POSIX launcher. Claude Code on
Windows is not verified for 1.0.

## Where the engine goes

`setup` installs `kmp-mcp` into `~/.local/bin` (`--engine-dir DIR` or
`KMP_INSTALL_DIR` to change it). A Claude Code-only setup uses the engine the
plugin owns. If `~/.local/bin` is not on your `PATH`, the commands on this
page need the full path, and `doctor` warns `no kmp-mcp is on PATH`.

## Check it worked

```bash
kmp-mcp doctor
```

or `/kmp:doctor` (Claude Code), `kmp-doctor` (Codex). Success ends with
`Usable.` or `Usable, with a warning above.`; the Hosts section names your
host's plugin with `ok`. Some warnings are expected: on a fresh machine, `this
memory has never been started here` and no quality journal yet, until the
first session; and `MCP registration installed and enabled, live connection
unverified`, because doctor cannot open a host session itself.

Then see something:

```bash
kmp-mcp demo
```

It writes a worked example into the store this directory selects and opens
ChronoLoom on it. It never writes a project's store: inside a project it writes
the store a directory outside any project would open, and says so.
[What to ask next](../examples/README.md).

## What setup downloads

The engine and the lexical-bridge table, from GitHub Releases, each checked
against the SHA-256 published beside it. Setup selects and writes no memory.
See [Security and privacy](security-privacy.md#setup-update-and-the-plugin-bootstrap).

## Uninstall

```bash
kmp-mcp uninstall            # preview: what would be removed
kmp-mcp uninstall --apply    # remove; a verified memory copy is saved first
```

`/kmp:uninstall` in Claude Code, `kmp-uninstall` in Codex.
