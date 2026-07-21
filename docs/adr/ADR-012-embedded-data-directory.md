# ADR-012: Data directory contract — env override, then per-project `.kernel/`, then per-user XDG

**Status:** Accepted
**Date:** 2026-07-21
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), milestone E0

## Decision

The embedded backend resolves its data directory at startup, in this
precedence order, and logs which rule won:

1. **Explicit override:** `REHYDRATION_MCP_DATA_DIR` (absolute path). Set →
   used as-is, created if missing. This is also the escape hatch for hosts
   that launch MCP servers with an unpredictable working directory.
2. **Per-project (the default experience):** walk up from the process
   working directory looking for a `.git` entry; at the first match, use
   `<project-root>/.kernel/`. Created on first write, together with a
   `.kernel/.gitignore` containing `*` (cargo-`target`-style self-ignoring
   directory), so project memory stays local-first and never enters version
   control by accident.
3. **Per-user fallback (no project root found):** the platform user data
   directory — `$XDG_DATA_HOME/rehydration-kernel/default` (Linux, with the
   usual `~/.local/share` fallback) and its macOS/Windows equivalents via a
   platform-dirs crate.

## Data directory layout

```
<data-dir>/
  FORMAT_VERSION   # single integer, stamped at creation
  lock             # advisory lock file (ADR-011)
  store/
    kernel.redb    # the storage engine file (ADR-009)
  logs/            # local structured logs, rotated (E3) — stdout stays MCP-only
```

Contract rules:

- **Fail-fast integrity:** `FORMAT_VERSION` newer than the binary supports →
  explicit "binary too old" error; older → explicit "run --migrate" error;
  file missing while `store/` exists → explicit corrupt-layout error. Never
  silent empty memory (roadmap non-negotiable).
- **Nothing is auto-deleted.** Migration and compaction write new files and
  keep the event log; destructive cleanup is always an explicit user command.
- **One store per data dir.** Scoping (which project, which user) is decided
  entirely by which directory is resolved — the store itself has no
  multi-tenancy.

## Why

- **Per-project is the product.** The roadmap's target use case is
  per-repo agent memory; keys derived from project paths in a shared user
  dir (the rejected alternative) survive `git clone` to a new path worse and
  are invisible next to the repo. A visible `.kernel/` directory is
  self-explaining, `du`-able, trivially backed up or deleted with the
  checkout, and travels when the working copy is moved or copied.
- **`.git` as the project marker** matches how agent hosts (Claude Code,
  Codex CLI) define a project and needs no configuration. Worktrees resolve
  to their own root (a worktree's `.git` file is found first), keeping
  worktree memory isolated — consistent with the single-writer model
  (ADR-011) since worktrees are typically parallel sessions.
- **Env-first** keeps the promotion story one variable away
  (`REHYDRATION_MCP_BACKEND` switches edition, `REHYDRATION_MCP_DATA_DIR`
  pins storage), and makes CI/tests hermetic.
- **XDG fallback** means launching the binary outside any repo still yields
  working, durable memory rather than an error — the "one developer, many
  agent sessions" case with a personal store.

## Consequences

- **Positive:** zero-config default that puts memory where the user's mental
  model is (next to the code); explicit override for everything else.
- **Positive:** self-gitignoring directory prevents the classic
  memory-committed-to-repo accident without asking users to edit
  `.gitignore`.
- **Trade-off:** `.kernel/` contents are lost with a deleted checkout.
  Accepted: that is the expected lifecycle of per-project local memory, and
  the export/import bundle path (E6) is the durability story across
  checkouts.
- **Trade-off:** resolution depends on process CWD when the env var is
  unset; hosts that launch servers from `$HOME` would silently fall back to
  the user store. Mitigated: the E4 per-host recipes must pin either CWD or
  `REHYDRATION_MCP_DATA_DIR`, and the resolved directory + winning rule are
  logged at startup.

## Next Step

E2 implements resolution + layout + fail-fast checks in the embedded
composition surface; E4 recipes pin the resolution per host; E5 documents the
`FORMAT_VERSION` ↔ binary compatibility matrix.
