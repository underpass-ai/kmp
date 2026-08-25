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
claude mcp add kmp --scope user \
  -- ~/.cargo/bin/kmp-mcp
```

`--scope user` registers it for every project; each project still gets its
own `.kernel/` store. Verify with `claude mcp list` and, inside a session,
call `kmp_wake` on any about you have written.

## Codex CLI — tested 2026-07-23

`~/.codex/config.toml` (applied and verified in a live Codex session:
`kmp_wake` recovered a real checkpoint with proof). Two field notes from
the verification: sessions started before a registration change keep the old
MCP inventory (restart the session), and opening a host in a project whose
`.kernel/` is held by another session fails fast per the single-writer
contract — the tools then do not appear in the inventory (open the host in a
project with a free store, or close the other session):

```toml
[mcp_servers.kmp]
command = "/home/YOU/.cargo/bin/kmp-mcp"
# no env: the embedded kernel is the default
```

## OpenCode — recipe (out of initial product scope)

Project or global config (`opencode.json`):

```json
{
  "mcp": {
    "kmp": {
      "type": "local",
      "command": ["/home/YOU/.cargo/bin/kmp-mcp"],
      "environment": {}
    }
  }
}
```

## GitHub Copilot (VS Code agent mode) — recipe (pending verification)

`.vscode/mcp.json`:

```json
{
  "servers": {
    "kmp": {
      "command": "/home/YOU/.cargo/bin/kmp-mcp",
      "env": {}
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
- On session start for known work, call `kmp_wake {about}` before
  re-deriving context; abouts follow `project:<name>` / `incident:<id>`.
- Ask targeted questions with `kmp_ask`; navigate history with
  `kmp_goto/near/rewind/forward`; audit claims with `kmp_inspect`.
- Write memory when you decide, constrain, or conclude — decisions,
  constraints, outcomes with coordinates and evidence. Never transcripts.
- One `idempotency_key` per logical write; a conflict on retry means
  "already applied".
- A decision made in another project is read with
  `dimensions: {scope: "abouts", abouts: [...]}` — see below.
```

## Reading one project's memory from another project's conversation

Abouts are never joined by relations, and that is deliberate: an edge between
two projects bakes the link into the graph, so anyone traversing one drags the
other along whether they want it or not, and the frontier an about exists to
bound stops being bounded.

The join therefore lives with the reader, at read time:

```json
{
  "about": "project:made",
  "question": "Why does the store conversion copy rows instead of replaying the journal?",
  "dimensions": { "scope": "abouts", "abouts": ["project:made", "project:kmp"] }
}
```

One call, both projects, evidence attributed to each. The reasoning recorded
in `project:kmp` arrives in a conversation about MADE, and neither graph grows
an edge it did not ask for.

Reach for it whenever a decision made in one project governs work in another —
a shared contract, an ADR both sides implement, a constraint one repository
imposes on its sibling. `scope: "all_abouts"` sweeps everything and is a real
cost on a large store; naming the two or three abouts that bear on the
question costs almost nothing.

## Two sessions at once

Fresh stores from the shipped MCP binary use SQLite and support several
hosts. An existing redb store remains single-writer (ADR-011); move that store
to SQLite (ADR-018) when Claude Code and Codex CLI need the same memory:

```bash
kmp-mcp migrate ~/.local/share/kmp/default ~/.local/share/kmp/shared --engine sqlite
```

Current crates.io and plugin builds already carry SQLite. The full recipe is
in [mcp-stdio.md](mcp-stdio.md#sharing-one-memory-between-two-agent-hosts).

## Scripted acceptance demo

`scripts/demo/embedded_two_sessions.sh` — three independent processes on
one data dir: write a decision, recover it, audit it with proof. Run it
after any change to the embedded backend:

```bash
bash scripts/demo/embedded_two_sessions.sh
```
