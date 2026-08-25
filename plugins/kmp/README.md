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

**Claude Code** — native install from the marketplace. The manifest lives in
[underpass-ai/plugins](https://github.com/underpass-ai/plugins), which carries
both Underpass plugins, so the same source also offers `made@underpass`:

```
/plugin marketplace add underpass-ai/plugins
/plugin install kmp@underpass
```

A marketplace install brings the skill, the commands and the launcher, but not
the binary — `bin/kmp-mcp` is gitignored, so it exists only in a release
package. That is what `/kmp:setup` is for: it downloads the engine matching
this plugin's version, from the release that published it, verified against
the checksum published beside it, and no Rust toolchain is involved.

```text
/kmp:setup
```

The launcher looks for `bin/kmp-mcp` first, so a release bundle keeps its
pinned binary, and otherwise falls back to `kmp-mcp` on `PATH` — which is
where `/kmp:setup` puts it. If neither exists the launcher fails with an
explicit message naming both places it looked.

`cargo install kmp-mcp` remains the fallback for a platform with no published
asset, and a release package remains the way to install a pinned pair with no
download step at all.

### Catching up

The session-start hook checks GitHub Releases at most once per day. It is
silent when the plugin and engine are current, and fail-open when offline. If
both halves are two releases behind together, it still notices — equality is
not mistaken for freshness — and offers one command:

```text
/kmp:setup
```

Setup runs `scripts/kmp-update.sh`: Claude's native plugin update plus the
checksummed engine from the same release. Codex uses `/kmp-setup`, which
refreshes its prompts and doctrine from the versioned release as well. Both
paths finish with one restart because a running host keeps the MCP inventory
it started with.

**Codex CLI** — no plugin system, so a script does the wiring:

```bash
bash scripts/mcp/install-kmp-plugin.sh --codex
```

It installs the binary if missing, registers `[mcp_servers.kmp]` in
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

- **recover before re-deriving** — `kmp_wake {about}` before reading files
  to reconstruct context that may already be stored;
- **write decisions, constraints and outcomes — never transcripts**;
- **rich relations carry both `why` and `evidence`**: the first explains the
  semantic connection and the second proves that rationale;
- `UNKNOWN` from `kmp_ask` is a correct answer, not a failure to route
  around.

The skill points at `tools/list` as the authority on the relation vocabulary,
because that catalog is generated from the kernel's own writer spec and moves
with the kernel. The skill teaches the shape; the schema carries the truth.

The payoff appears on the read path: `kmp_wake` reconstructs the causal
spine, `kmp_ask` can keep the right citation when the question is
paraphrased, and `kmp_trace` / `kmp_inspect` expose the original
rationale and proof verbatim. KMP uses what the writer supplied; it never
generates a missing `why`. See
[Why the `why` matters](skills/kmp-memory/SKILL.md#why-the-why-matters) for the
field-by-field model, safe fallbacks and worked examples.

### For you — nine commands

| Command | What it does |
| --- | --- |
| `/kmp:setup` | Installs and wires whatever is missing, then re-checks |
| `/kmp:doctor` | Diagnoses the setup end to end and names the one thing to fix |
| `/kmp:info` | What this install is and which memory this project opens — and why that one |
| `/kmp:moves` | The ten moves and when to use each, read from the live surface when reachable |
| `/kmp:demo` | Loads an example memory — a real incident with a wrong turn in it |
| `/kmp:catchup` | What changed since you last looked, from the event log |
| `/kmp:save` | Commits this project's memory to the repository, and shows the diff |
| `/kmp:restore` | Loads the memory committed in the repository back into the store |
| `/kmp:revert` | Reverts a decision without deleting it, so both states survive |

Codex gets all nine as `/kmp-setup`, `/kmp-doctor` and so on. They read the
same because they are held to the same standard: [VOICE.md](VOICE.md) is the
source of truth for how KMP talks, and `scripts/ci/kmp-plugin-voice.sh` fails
the build when a command drifts out of it.

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
holding a redb store, which is the single-writer contract (ADR-011) doing its
job — the doctor says which engine the store is on and names `share-memory`,
which snapshots, migrates and verifies it with the SQLite engine already
carried by current bundles — and a session that
started before the registration changed and is still carrying the old
inventory. The second one cannot be fixed from inside the session — you have
to restart it.

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
