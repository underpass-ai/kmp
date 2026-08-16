# KMP plugin — discovery for agents and humans

Wires KMP agent memory into a coding agent and makes it discoverable from
both sides: the agent learns *when* to reach for memory, and you get commands
to see the surface and to find out why it is not working.

Without this plugin, using KMP means installing a binary, copying an MCP
registration into your host's config, and pasting a context-recovery playbook
into `CLAUDE.md` or `AGENTS.md` by hand. The plugin does all three.

## Install

**From a release package** — each GitHub Release attaches
`kmp-plugin-<version>-<os>-<arch>.tar.gz` with a per-archive `.sha256`
checksum. The bundle is self-contained: it carries the `kmp-mcp` binary in
`bin/`, both host manifests, the skill, the commands and the launcher
scripts. Verify, unpack, and point the host at the resulting `kmp/`
directory:

```bash
sha256sum -c kmp-plugin-<version>-<os>-<arch>.sha256
tar -xzf kmp-plugin-<version>-<os>-<arch>.tar.gz
```

Codex reads `.codex-plugin/plugin.json`, Claude Code reads
`.claude-plugin/plugin.json`, and both start the MCP server through
`.mcp.json` → `scripts/run-embedded-mcp.sh`. On Windows hosts, register
`scripts\run-embedded-mcp.cmd` instead. To build the package from a
checkout: `bash scripts/plugin/package-kmp-plugin.sh`.

**Claude Code** — native install from the marketplace:

```
/plugin marketplace add underpass-ai/kmp
/plugin install kmp@underpass
```

This installs the skill and the commands. It does **not** bring the binary: the
launcher runs `bin/kmp-mcp` inside the plugin directory, and that path is
gitignored, so it exists only in a release package or a locally built bundle.
After a marketplace install, run `/kmp:setup` — it diagnoses what is missing and
wires the `kernel-memory` server against an installed `kmp-mcp`.

**Codex CLI** — no plugin system, so a script does the wiring:

```bash
bash scripts/mcp/install-kmp-plugin.sh --codex
```

It installs the binary if missing, registers `[mcp_servers.kernel-memory]` in
`~/.codex/config.toml`, drops `/kmp-doctor` and `/kmp-moves` into
`~/.codex/prompts/`, and adds the memory doctrine to `~/.codex/AGENTS.md`.
Re-running is safe: it reports what is already wired instead of duplicating
it, and the `AGENTS.md` section is fenced by markers so it is replaced rather
than stacked. Pass `--dry-run` to see the changes before making them.

The script works outside a checkout too, fetching what it needs from the
repository.

## What you get

### For the agent — the `kmp-memory` skill

Loads when the work continues something earlier, or when a decision worth
remembering is reached. It carries the operating doctrine:

- **recover before re-deriving** — `kernel_wake {about}` before reading files
  to reconstruct context that may already be stored;
- **write decisions, constraints and outcomes — never transcripts**;
- **rich relations carry both `why` and `evidence`**, so a vague `related_to`
  is a bug rather than a shortcut;
- `UNKNOWN` from `kernel_ask` is a correct answer, not a failure to route
  around.

The skill points at `tools/list` as the authority on the relation vocabulary,
because that catalog is generated from the kernel's own writer spec and moves
with the kernel. The skill teaches the shape; the schema carries the truth.

### For you — three commands

| Command | What it does |
| --- | --- |
| `/kmp:doctor` | Diagnoses the setup end to end and tells you the one thing to fix |
| `/kmp:moves` | The ten moves and when to use each, read from the live surface when reachable |
| `/kmp:setup` | Installs and wires whatever is missing, then re-checks |

In Codex: `/kmp-doctor` and `/kmp-moves`.

## The doctor

`/kmp:doctor` exists because the failure modes are specific, and they all
look identical from inside a session — the `kernel_*` tools are simply not
there. It separates them:

- **binary** — installed, on `PATH`, and its version;
- **backend** — embedded, grpc or fixture. It flags `fixture` loudly: those
  responses look real and are canned;
- **data directory** — which one wins under the ADR-012 resolution order
  (`KMP_MCP_DATA_DIR` → project `.kernel/` → XDG fallback), and why;
- **tool surface** — a real `tools/list` over stdio, counting what answers;
- **host registration** — whether Claude Code and Codex actually have it.

Two failures it names rather than leaving you to guess: another session
holding the store, which is the single-writer contract (ADR-011) doing its
job, and a session that started before the registration changed and is still
carrying the old inventory. The second one cannot be fixed from inside the
session — you have to restart it.

Run it directly, without a host:

```bash
bash plugins/kmp/scripts/kmp-doctor.sh
```

## Backends

The plugin registers the **embedded** backend: the kernel runs inside the
binary, storage is a local `.kernel/` directory per project, no
infrastructure. For a shared deployed kernel, point the server at it with
`KMP_KERNEL_GRPC_ENDPOINT` instead — the tool surface is identical by
construction, so nothing else changes.

See [mcp-stdio.md](../../docs/operations/mcp-stdio.md) for the full mode
matrix and [embedded-hosts.md](../../docs/operations/embedded-hosts.md) for
per-host recipes, including hosts this plugin does not cover.
