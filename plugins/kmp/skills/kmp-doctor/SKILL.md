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
