---
name: kmp-doctor
description: Diagnose KMP binary, backend, selected store, tool surface, host wiring, and every kmp-mcp engine on this machine. Use when memory or KMP setup is missing, stale, duplicated, or failing.
---

# KMP doctor

Resolve the plugin root as two directories above this `SKILL.md` and run
`<plugin-root>/scripts/kmp-doctor.sh`. This is the primary doctor because it
checks the binary and store plus effective Claude/Codex wiring, retired tool
policies, and plugin/global MCP ownership.

Only if the bundled script is unavailable, fall back to `kmp-mcp doctor` and
state explicitly that host wiring and ownership were not checked.

Show the first branded block verbatim. Then give the usable/not-usable verdict,
the first blocking cause, and the exact repair named by the doctor. Preserve
warnings about fixture mode, stale sessions, retired tool-name policies,
plugin/global MCP ownership collisions, and a lexical-bridge table the store
ignored: a malformed table means Ask quietly stopped crossing languages.

Keep three host facts apart when you report them; the doctor prints them as
three separate lines and collapsing them hides the failure:

- the plugin is **installed and enabled** in that host;
- the **engine binary declares** its tools, proved by running the executable
  directly against a scratch directory — not evidence of a host connection;
- a host reports a **live MCP connection**. Only that line is verification.
  Where no live probe ran, the doctor says *unverified*, which is a warning,
  not an approval. Report it as unverified: a registration an inventory knows
  about is not a conversation that can call a tool.

The Memory section names the winning selection rule and the whole precedence.
When the selected memory cannot be opened, report the reason and the repair the
doctor printed. Never suggest migrating or replacing a store: KMP refuses an
incompatible one and leaves every file in it untouched, and choosing another
directory with `kmp-mcp config memory-store <absolute-path>` — or
`--clear` for automatic selection — is the user's decision to make.
