# KMP plugin — discovery for agents and humans

<!-- kmp:public-overview:begin -->
KMP gives Codex and Claude Code local-first memory that preserves what
happened, when and why. It stores decisions and evidence, not transcripts,
on embedded SQLite, and exposes them through ten memory tools plus three
semantic view tools over a shared ChronoLoom view.

Ask **“Show me the memory behind this decision.”** The agent retrieves the
evidence, opens ChronoLoom at the relevant moment and lights up the proof path.
You can click, filter, pan, undo or take control of the same view at any time.
<!-- kmp:public-overview:end -->

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

Codex reads `.codex-plugin/plugin.json` and starts `kmp-mcp` from `PATH`.
Claude Code reads
`.claude-plugin/plugin.json` and starts through `.mcp.json` →
`scripts/run-embedded-mcp.sh`, which can prefer the binary bundled in a
release package. Put the bundled `bin/` on `PATH` for Codex. On Windows hosts,
Claude uses `scripts\run-embedded-mcp.cmd`. To build the package from a checkout:
`bash scripts/plugin/package-kmp-plugin.sh`.

**Claude Code** — native install from KMP's co-located stable marketplace:

```
/plugin marketplace add underpass-ai/kmp@marketplace
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
codex plugin marketplace add underpass-ai/kmp --ref marketplace
codex plugin add kmp@underpass
```

Run the native `kmp-setup` skill to install the matching engine and diagnose
the result. Re-running it preserves the plugin as the single MCP owner.

The native plugin is the only supported Codex owner. Do not add a second
global `mcp_servers.kmp` table or copy prompts and AGENTS fragments beside it.

### Agent routing policy

`kmp-mcp config` shows the policy agents receive at MCP initialization and the
file that owns it.

The policy opens with a gate. By default memory is **opt-in**: an agent calls a
kmp tool when the user asks for KMP or its memory, when a kmp skill or command
runs, or when the project's own instructions opt in — and otherwise makes no
call. Everything after the gate governs a route already underway. An operator
who wants known work to enter through `kmp_wake` unprompted asks for it:

```bash
kmp-mcp config memory-routing always
# or back to opt-in
kmp-mcp config memory-routing on-request
```

With a lexical-bridge table beside the store, `kmp_ask` crosses languages
inside the kernel and the retry is not used. Without a table and with no
policy file, genuinely semantic `kmp_ask` calls get one bounded English retry
after the user's-language question returns `UNKNOWN`. Configure a different
ordered list during setup with:

```bash
kmp-mcp config ask-fallback-languages en,fr
# or disable retries
kmp-mcp config ask-fallback-languages none
```

A writer can also attach an English rendering of a memory as the reserved
entry metadata key `summary_en`. `kmp_ask` searches it and never cites it, so
an English question reaches Spanish memory and the citation stays the Spanish
text byte for byte. `kmp_ingest` warns about a summary the lint refuses —
wrong language, too thin, a copy of the text, a dropped identifier — and
ranking makes the same reading, so such a summary carries nothing. A citation
the summary carried says so: `matched_via: summary`, with the question's words
the rendering supplied in `summary_terms`. `kmp_write_memory` takes it as
`current.summary_en`, and a strict write requires it when the memory is not
written in English.

Fallback tags whose primary language is `zh`, `ja`, or `th` are rejected until
Ask supports word segmentation for those scripts. Storage remains byte-exact;
word-based semantic retrieval in those languages is not supported yet.

The fallback translates only the query. The answer follows the user's
language; evidence text, refs, relation `why`, and source metadata remain
byte-for-byte as stored. Temporal requests such as “yesterday” or a release
window bypass semantic Ask: the agent resolves the user's timezone, navigates
the half-open UTC interval and consumes every page. It captures the inclusive
start with `kmp_goto` before the strictly-after `kmp_forward`, merges refs and
excludes the end. Setup and upgrades preserve the configured list.

Setup and update install the versioned guide assets but never select or write
a memory store. `/kmp:guide` performs the separate, explicit sync:
`guide:kmp-agent` explains every live verb to the agent and `guide:kmp` is the
short human path. The sync is deterministic, but it writes to the selected
store; in a project it changes project memory and `.kmp/memory.jsonl`.

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

An explicit `/kmp:guide` synchronizes `guide:kmp-agent`, a versioned operating
guide inside KMP itself. Its explicit verb cards say when to use a move, when
not to, the minimum input, the expected result and the normal next move. The
separate `guide:kmp` is for people and opens visually in ChronoLoom; it is not
a shorter set of agent instructions.

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
| `/kmp:moves` | The memory and ChronoLoom moves, read from the live surface when reachable |
| `/kmp:guide` | Syncs the agent guide, then runs `open:guide` on the separate human path in ChronoLoom |
| `/kmp:catchup` | What changed since you last looked, from the event log |
| `/kmp:save` | Commits this project's memory to the repository, and shows the diff |
| `/kmp:restore` | Loads the memory committed in the repository back into the store |
| `/kmp:revert` | Reverts a decision without deleting it, so both states survive |
| `/kmp:uninstall` | Previews whole-install or one-store removal, refuses live owners, protects memory first, and only applies when explicitly asked |

Codex gets all ten as native `kmp-setup`, `kmp-doctor` and so on. Standalone
Codex keeps the equivalent `/kmp-*` prompts. Claude Code keeps `/kmp:*`
commands. [`capabilities.json`](capabilities.json) is the machine-checked
inventory that maps each workflow to its owner and exposure; the ten memory
tools sit inside the separate thirteen-tool MCP contract. [VOICE.md](VOICE.md)
remains the source of truth
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

For Codex it also detects obsolete global wiring. If an enabled plugin and a
global `mcp_servers.kmp` table both claim the server, Doctor names both owners
and the global store-affecting environment instead of declaring the setup
healthy.

Two failures it names rather than leaving you to guess: an unsupported store
format, which current KMP detects but deliberately does not open — Doctor
points to the external archive-export contract and leaves the bytes untouched
— and a session that started before the registration changed and is still
carrying the old inventory. The second one cannot be fixed from inside the
session; you have to restart it.

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
