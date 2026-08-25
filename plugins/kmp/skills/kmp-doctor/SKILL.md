---
name: kmp-doctor
description: Diagnose KMP binary, backend, selected store, tool surface, and host wiring. Use when memory or KMP setup is missing, stale, duplicated, or failing.
---

# KMP doctor

Run `kmp-mcp doctor`. If the plugin-bundled doctor is needed, resolve the
plugin root as two directories above this `SKILL.md` and run
`<plugin-root>/scripts/kmp-doctor.sh`.

Show the first branded block verbatim. Then give the usable/not-usable verdict,
the first blocking cause, and the exact repair named by the doctor. Preserve
warnings about fixture mode, stale sessions, retired tool-name policies, and
plugin/global MCP ownership collisions.
