<!-- kmp:begin -->
## Kernel memory (KMP)

Graph-temporal memory over the `kmp` MCP server. Every answer is
derived from stored evidence; nothing here generates prose. `kmp_ask`
returning `UNKNOWN` is a correct result, not a failure to work around.

- **Recover before re-deriving.** On session start for known work, call
  `kmp_wake {about}` before reading files to reconstruct context. Abouts
  are stable ids: `project:<name>`, `incident:<id>`. An empty wake packet
  means there is no memory yet — start writing one.
- **Route before retrieving; temporal intent wins.** `yesterday`, `today`,
  `since`, `before`, `after`, `during`, explicit dates/timestamps and release
  windows — in the user's language — use `kmp_goto` / `kmp_near` /
  `kmp_rewind` / `kmp_forward` before `kmp_ask`. Resolve the user's timezone
  to a half-open UTC interval `[start, end)` and follow temporal pagination to
  completion or report its exact continuation.
- **Ask semantically, with bounded language fallback.** For a non-temporal
  question, `kmp_ask` in the user's language first. On `UNKNOWN` or evidence
  that does not answer, retry once per fallback language configured by
  `kmp-mcp config`. Translate only the query, answer in the user's language,
  and preserve stored evidence, refs, relation `why` and source metadata
  byte-for-byte. `UNKNOWN` remains valid after the bounded retries.
- **Then audit.** `kmp_trace` proves a path between refs; `kmp_inspect` shows
  one ref's object, links and evidence.
- **Write decisions, constraints and outcomes — never transcripts.** Memory
  is the durable shape of the work, not a log of the conversation. Prefer
  `kmp_write_memory`, which validates intent and relation quality before
  compiling to canonical ingest; use `options.dry_run=true` to see what a
  write would commit.
- **Relations carry the why.** `why` explains why the specific semantic link
  holds; `evidence` is the concrete observation or source that proves that
  rationale. KMP preserves and uses this context in wake, recall and audit,
  but never generates it. Rich relations require both fields. If context
  cannot justify one, use the honest fallback `follows`/procedural,
  `answers`/evidential or `uses_background`/evidential. The full guide is in
  the `kmp-memory` skill, section “Why the `why` matters”; `tools/list` is the
  authority for the current relation vocabulary.
- **One `idempotency_key` per logical write.** A conflict on retry means the
  write was already applied. That is success.
- **Scope is explicit.** Omitted means `current_about`; `abouts` needs a
  non-empty list; `all_abouts` traverses everything and is a real cost.
- **If the tools are missing, say so** instead of silently re-deriving
  everything. Usual causes: the binary is not on `PATH`, another session
  holds this project's `.kernel/` store (single-writer, ADR-011), or the
  session started before the MCP registration changed. Run `/kmp-doctor`.
<!-- kmp:end -->
