# Embedded Edition: Host Registration Recipes (E4)

How to give a coding agent persistent kernel memory with one command.
Status per host is explicit: **tested** means executed against the real
host on a real machine; **recipe** means derived from the host's MCP
documentation and pending verification.

Install once:

```bash
cargo install --path crates/rehydration-mcp --locked   # → ~/.cargo/bin/rehydration-mcp
```

Without `REHYDRATION_MCP_DATA_DIR`, memory is **per-project by default**:
the binary walks up from its working directory to the `.git` root and keeps
memory in `<project>/.kernel/` (auto-gitignored). Set the variable only to
pin a fixed store.

## Claude Code — tested 2026-07-23

```bash
claude mcp add kernel-memory --scope user \
  --env REHYDRATION_MCP_BACKEND=embedded \
  -- ~/.cargo/bin/rehydration-mcp
```

`--scope user` registers it for every project; each project still gets its
own `.kernel/` store. Verify with `claude mcp list` and, inside a session,
call `kernel_wake` on any about you have written.

## Codex CLI — registered 2026-07-23 (session-level verification pending)

`~/.codex/config.toml` (applied on this machine; note: Codex sessions started
before the change keep the old MCP inventory — restart the session):

```toml
[mcp_servers.kernel-memory]
command = "/home/YOU/.cargo/bin/rehydration-mcp"
env = { REHYDRATION_MCP_BACKEND = "embedded" }
```

## OpenCode — recipe (out of initial product scope)

Project or global config (`opencode.json`):

```json
{
  "mcp": {
    "kernel-memory": {
      "type": "local",
      "command": ["/home/YOU/.cargo/bin/rehydration-mcp"],
      "environment": { "REHYDRATION_MCP_BACKEND": "embedded" }
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
      "command": "/home/YOU/.cargo/bin/rehydration-mcp",
      "env": { "REHYDRATION_MCP_BACKEND": "embedded" }
    }
  }
}
```

## Context-recovery playbook (paste into CLAUDE.md / AGENTS.md / rules)

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

The embedded store is single-writer (ADR-011): a second host session on the
same data dir fails fast with an explicit "store is in use" error rather
than corrupting memory. Close the other session, or point the second one at
a different `REHYDRATION_MCP_DATA_DIR`. If concurrent sessions on one store
become a common workflow, the documented evolution is a local daemon.

## Scripted acceptance demo

`scripts/demo/embedded_two_sessions.sh` — three independent processes on
one data dir: write a decision, recover it, audit it with proof. Run it
after any change to the embedded backend:

```bash
bash scripts/demo/embedded_two_sessions.sh
```
