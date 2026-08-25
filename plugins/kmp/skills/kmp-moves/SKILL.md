---
name: kmp-moves
description: Explain the live ten-tool KMP MCP surface and relation vocabulary. Use when the user asks what KMP can do or which move fits a task.
---

# KMP moves

Prefer the live `tools/list` result. It must expose exactly:
`kmp_ingest`, `kmp_write_memory`, `kmp_wake`, `kmp_ask`, `kmp_goto`,
`kmp_near`, `kmp_rewind`, `kmp_forward`, `kmp_trace`, and `kmp_inspect`.

Group them as entry (`wake`, semantic `ask`), time (`goto`, `near`, `rewind`,
`forward`), audit (`trace`, `inspect`), and write (`write_memory`, low-level
`ingest`). Use `tools/list` as the authority for relation vocabulary. State
that temporal intent uses the time group before semantic Ask.
