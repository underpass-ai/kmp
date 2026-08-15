---
description: Install and wire KMP agent memory for this machine (binary, Claude Code, Codex CLI)
argument-hint: "[--codex] [--claude]"
---

Get KMP memory working on this machine. Diagnose first, then fix only what is
actually missing — do not reinstall what already works.

Start with:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-doctor.sh"
```

Then act on what it reported.

**Binary missing** — install it, and prefer the repository helper when the
user is inside a checkout, because it pins refs:

```bash
cargo install kmp-mcp
# inside a checkout, to pin refs:
bash scripts/mcp/install-kmp-mcp.sh
# for the unreleased tip:
cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
```

**Claude Code not wired** — if this plugin is installed, the `kernel-memory`
server ships with it and no separate registration is needed; a stale session
is the likely cause, so restart it. Register manually only if the user wants
the server without the plugin:

```bash
claude mcp add kernel-memory --scope user \
  --env KMP_MCP_BACKEND=embedded -- "$(command -v kmp-mcp)"
```

**Codex CLI not wired** — the installer is idempotent and safe to re-run:

```bash
bash scripts/mcp/install-kmp-plugin.sh --codex
```

It writes `[mcp_servers.kernel-memory]` into `~/.codex/config.toml` and drops
the `/kmp-doctor` and `/kmp-moves` prompts into `~/.codex/prompts/`.

If `$ARGUMENTS` names a host, restrict the work to that host.

Finish by re-running the doctor and telling the user whether memory is now
answering. If the only thing left is a stale session, say that plainly — it
is the one fix that has to happen outside this session.

One thing worth flagging while you are here: the embedded store is
single-writer (ADR-011). If the user runs Claude Code and Codex in the same
project at once, the second one gets no tools. That is the contract working,
not a bug — different projects, or a different `KMP_MCP_DATA_DIR`.
