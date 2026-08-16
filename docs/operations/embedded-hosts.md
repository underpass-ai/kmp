# Embedded Edition: Host Registration Recipes (E4)

How to give a coding agent persistent kernel memory with one command.
Status per host is explicit: **tested** means executed against the real
host on a real machine; **recipe** means derived from the host's MCP
documentation and pending verification.

For Claude Code and Codex CLI, the [KMP plugin](../../plugins/kmp/README.md)
performs the registration below for you and adds the discovery aids — the
agent-facing skill and `/kmp:doctor`. The recipes here stay authoritative for
every other host, and for registering the server without the plugin.

Install once:

```bash
cargo install --path crates/kmp-mcp --locked   # → ~/.cargo/bin/kmp-mcp
```

Without `KMP_MCP_DATA_DIR`, memory is **per-project by default**:
the binary walks up from its working directory to the `.git` root and keeps
memory in `<project>/.kernel/` (auto-gitignored). Set the variable only to
pin a fixed store.

## Claude Code — tested 2026-07-23

```bash
claude mcp add kernel-memory --scope user \
  --env KMP_MCP_BACKEND=embedded \
  -- ~/.cargo/bin/kmp-mcp
```

`--scope user` registers it for every project; each project still gets its
own `.kernel/` store. Verify with `claude mcp list` and, inside a session,
call `kernel_wake` on any about you have written.

## Codex CLI — tested 2026-07-23

`~/.codex/config.toml` (applied and verified in a live Codex session:
`kernel_wake` recovered a real checkpoint with proof). Two field notes from
the verification: sessions started before a registration change keep the old
MCP inventory (restart the session), and opening a host in a project whose
`.kernel/` is held by another session fails fast per the single-writer
contract — the tools then do not appear in the inventory (open the host in a
project with a free store, or close the other session):

```toml
[mcp_servers.kernel-memory]
command = "/home/YOU/.cargo/bin/kmp-mcp"
env = { KMP_MCP_BACKEND = "embedded" }
```

## OpenCode — recipe (out of initial product scope)

Project or global config (`opencode.json`):

```json
{
  "mcp": {
    "kernel-memory": {
      "type": "local",
      "command": ["/home/YOU/.cargo/bin/kmp-mcp"],
      "environment": { "KMP_MCP_BACKEND": "embedded" }
    }
  }
}
```

## GitHub Copilot (VS Code agent mode) — recipe (pending verification)

`.vscode/mcp.json`:

```json
{
  "servers": {
    "kernel-memory": {
      "command": "/home/YOU/.cargo/bin/kmp-mcp",
      "env": { "KMP_MCP_BACKEND": "embedded" }
    }
  }
}
```

## Context-recovery playbook (paste into CLAUDE.md / AGENTS.md / rules)

The [KMP plugin](../../plugins/kmp/README.md) ships this playbook already —
as the `kmp-memory` skill in Claude Code, and written into
`~/.codex/AGENTS.md` by the Codex installer. Paste it by hand only for hosts
the plugin does not cover.

```markdown
## Kernel memory (KMP)
- On session start for known work, call `kernel_wake {about}` before
  re-deriving context; abouts follow `project:<name>` / `incident:<id>`.
- Ask targeted questions with `kernel_ask`; navigate history with
  `kernel_goto/near/rewind/forward`; audit claims with `kernel_inspect`.
- Write memory when you decide, constrain, or conclude — decisions,
  constraints, outcomes with coordinates and evidence. Never transcripts.
- One `idempotency_key` per logical write; a conflict on retry means
  "already applied".
```

## Two sessions at once

On the default engine (redb) the store is single-writer (ADR-011): a second
host session on the same data dir fails fast with an explicit "store is in
use" error rather than corrupting memory. Close the other session, point the
second one at a different `KMP_MCP_DATA_DIR`, or — if two hosts sharing one
memory is your normal day, Claude Code and Codex CLI on the same project —
move the store to the sqlite engine (ADR-018), which several processes can
open at once:

```bash
kmp-mcp migrate ~/.local/share/kmp/default ~/.local/share/kmp/shared --engine sqlite
```

Needs a `kmp-mcp` built with `--features sqlite`; the full recipe, and what
the engine costs, is in [mcp-stdio.md](mcp-stdio.md#sharing-one-memory-between-two-agent-hosts).

## Scripted acceptance demo

`scripts/demo/embedded_two_sessions.sh` — three independent processes on
one data dir: write a decision, recover it, audit it with proof. Run it
after any change to the embedded backend:

```bash
bash scripts/demo/embedded_two_sessions.sh
```
