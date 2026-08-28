---
description: Install and wire KMP agent memory for this machine (binary, Claude Code, Codex CLI)
argument-hint: "[--codex] [--claude] [--ask-fallback-languages en,fr|none]"
---

Get KMP memory working on this machine. Diagnose first, then fix only what is
actually missing — do not reinstall what already works.

Start with:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-doctor.sh"
```

Then act on what it reported.

**A newer KMP release is available** — update the plugin and engine together.
This is the one-command catch-up path offered by the session-start notice:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-update.sh"
```

It asks the native Claude plugin manager for the latest marketplace package,
installs the engine from that same release with its published checksum, and
then asks for one restart. Do not update only one half.

**Binary missing, or older than these plugin files** — install the engine this
plugin version expects. The installer downloads the release binary for this
platform and verifies it against the checksum published beside it, so no Rust
toolchain is needed:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-install-binary.sh"
```

It installs into `~/.local/bin` by default; pass `--dir` for somewhere else.
If the platform has no published binary it falls back to `cargo install`.
Tell the user if `~/.local/bin` is not on their `PATH` — the installer says so,
and a binary the launcher cannot find is the same as no binary.

Other ways, when the user asks for them:

```bash
cargo install kmp-mcp --force
# inside a checkout, to pin refs:
bash scripts/mcp/install-kmp-mcp.sh
# for the unreleased tip:
cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
```

A version mismatch is worth fixing even when memory answers: the launcher
falls through to whatever `kmp-mcp` is on `PATH`, so a stale half keeps
working while the fixes that live in the other half are silently missing.

**Claude Code not wired** — if this plugin is installed, the `kmp`
server ships with it and no separate registration is needed; a stale session
is the likely cause, so restart it. Register manually only if the user wants
the server without the plugin:

```bash
claude mcp add kmp --scope user \
  -- "$(command -v kmp-mcp)"
```

**Codex CLI not wired** — install the native plugin. It owns the MCP server and
native skills; setup must not also write a global registration:

```bash
codex plugin add kmp@underpass
```

Only when the user explicitly requests a plugin-free advanced installation:

```bash
bash scripts/mcp/install-kmp-plugin.sh --codex --standalone
```

Refuse standalone setup while the KMP plugin is enabled. If both owners
already exist, name the collision before changing configuration.

If `$ARGUMENTS` names a host, restrict the work to that host.

**Agent language policy** — show it during setup:

```bash
kmp-mcp config
```

With no user config, semantic `kmp_ask` retries are bounded to English after
the question in the user's language returns `UNKNOWN`; temporal requests do
not use this fallback. If `$ARGUMENTS` supplies `--ask-fallback-languages`,
persist its comma-separated value (or `none`) with:

```bash
kmp-mcp config ask-fallback-languages <tags|none>
```

Answer in the user's language, but translate only the retry query. Never
translate or rewrite stored evidence, refs, relation `why`, or source metadata.
An upgrade must preserve the existing policy.

Finish by re-running the doctor and telling the user whether memory is now
answering. If the only thing left is a stale session, say that plainly — it
is the one fix that has to happen outside this session.

Claude Code and Codex may share the same embedded SQLite store. WAL and
optimistic concurrency keep independent agent processes safe; a missing tool
inventory in one host is a registration or stale-session problem, not an
expected store lock.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
