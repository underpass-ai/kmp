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

Codex reads `.codex-plugin/plugin.json` and starts the installed `kmp-mcp`
from its inline MCP declaration. Claude Code reads
`.claude-plugin/plugin.json` and starts through `.mcp.json` →
`scripts/run-embedded-mcp.sh`, which can prefer the binary bundled in a
release package. On Windows hosts, register `scripts\run-embedded-mcp.cmd`
instead. To build the package from a checkout:
`bash scripts/plugin/package-kmp-plugin.sh`.

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

Setup runs `scripts/kmp-update.sh`: the host's native plugin update plus the
checksummed engine from the same release. Codex uses the native `kmp-setup`
skill; it does not copy prompts, edit `AGENTS.md`, or add global MCP wiring.
Both paths finish with one restart because a running host keeps the MCP
inventory it started with.

**Codex CLI** — install the native plugin. It owns both the skills and the
PATH-based MCP declaration, so it does not need a second registration in
`~/.codex/config.toml`:

```bash
codex plugin marketplace add underpass-ai/plugins
codex plugin add kmp@underpass
```

Run the native `kmp-setup` skill to install the matching engine and diagnose
the result. Re-running it preserves the plugin as the single MCP owner.

Standalone Codex wiring remains available as an explicit advanced path. It
owns a global MCP table, copied prompts and a fenced AGENTS section, and must
not be combined with an enabled KMP plugin:

```bash
bash scripts/mcp/install-kmp-plugin.sh --codex --standalone
```

The script works outside a checkout too, fetching what it needs from the
repository. Pass `--dry-run` to preview its changes.

### Agent routing policy

`kmp-mcp config` shows the policy agents receive at MCP initialization and the
file that owns it. With no file, genuinely semantic `kmp_ask` calls get one
bounded English retry after the user's-language question returns `UNKNOWN`.
Configure a different ordered list during setup with:

```bash
kmp-mcp config ask-fallback-languages en,fr
# or disable retries
kmp-mcp config ask-fallback-languages none
```

The fallback translates only the query. The answer follows the user's
language; evidence text, refs, relation `why`, and source metadata remain
byte-for-byte as stored. Temporal requests such as “yesterday” or a release
window bypass semantic Ask: the agent resolves the user's timezone, navigates
the half-open UTC interval and consumes every page. It captures the inclusive
start with `kmp_goto` before the strictly-after `kmp_forward`, merges refs and
excludes the end. Setup and upgrades preserve the configured list.

## What you get

### For the agent — the `kmp-memory` skill

Loads when the work continues something earlier, or when a decision worth
remembering is reached. It carries the operating doctrine:

- **recover before re-deriving** — `kmp_wake {about}` before reading files
  to reconstruct context that may already be stored;
- **route every result, not only the first request** — an unanswered semantic
  Ask about current state, a release or decision history moves to temporal
  navigation, while a genuinely semantic question may end at `UNKNOWN`;
- **write decisions, constraints and outcomes — never transcripts**;
- **rich relations carry both `why` and `evidence`**: the first explains the
  semantic connection and the second proves that rationale;
- **audit what the answer relies on** — inspect a cited ref and trace a claimed
  connection rather than treating retrieval as proof of either.

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

### For you — ten commands

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
| `/kmp:uninstall` | Previews removal, protects memory first, and only applies when explicitly asked |

Codex gets all ten as native `kmp-setup`, `kmp-doctor` and so on. Standalone
Codex keeps the equivalent `/kmp-*` prompts. Claude Code keeps `/kmp:*`
commands. [`capabilities.json`](capabilities.json) is the machine-checked
inventory that maps each workflow to its owner and exposure; MCP tools remain
a separate ten-verb contract. [VOICE.md](VOICE.md) remains the source of truth
for how the host workflows talk.

## The doctor

`/kmp:doctor` exists because the failure modes are specific, and they all
look identical from inside a session — the `kmp_*` tools are simply not
there. It separates them:

- **binary** — installed, on `PATH`, and its version;
- **backend** — embedded, grpc or fixture. It flags `fixture` loudly: those
  responses look real and are canned;
- **data directory** — which one wins under the ADR-012 resolution order
  (`KMP_MCP_DATA_DIR` → project `.kernel/` → XDG fallback), and why;
- **tool surface** — a real `tools/list` over stdio, counting what answers;
- **host registration** — whether Claude Code and Codex actually have it.

For Codex it also distinguishes plugin-managed from standalone wiring. If an
enabled plugin and a global `mcp_servers.kmp` table both claim the server, the
doctor names both owners and the global store-affecting environment instead of
declaring the setup healthy.

Two failures it names rather than leaving you to guess: another session
holding a legacy redb store, which is the single-writer contract (ADR-011) doing its
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

See [Embedded KMP](https://github.com/underpass-ai/kmp/blob/main/docs/embedded/README.md)
for the current local mode, storage and maintenance contract.
