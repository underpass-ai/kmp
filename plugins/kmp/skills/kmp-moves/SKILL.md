---
name: kmp-moves
description: Explain the live KMP MCP surface and relation vocabulary. Use when the user asks what KMP can do or which move fits a task.
---

# KMP moves

Prefer the live `tools/list` result; it is the surface, and a count written
down here only drifts. Its memory tools are `kmp_ingest`, `kmp_write_memory`,
`kmp_wake`, `kmp_ask`, `kmp_relate`, `kmp_goto`, `kmp_near`, `kmp_rewind`,
`kmp_forward`, `kmp_trace`, `kmp_inspect`, `kmp_relabel`, `kmp_condense` and
`kmp_summaries_audit`.
`kmp_guide` manages the persistent agent identity and progressive scheme.
Its three semantic view tools are `kmp_view_open`,
`kmp_view_apply_intent`, and `kmp_view_get_state`.

Group them as entry (`wake`, semantic `ask`, cross-about `relate`), time
(`goto`, `near`, `rewind`, `forward`), audit (`trace`, `inspect`), and write
(`write_memory`, `relabel` for the labels of a memory that exists, low-level
`ingest`, `summaries_audit` to see which memories an English question cannot
reach before writing their renderings), with the view tools controlling
ChronoLoom's semantic state. Use
`tools/list` as the authority for relation vocabulary. State that temporal
intent uses the time group before semantic Ask.
