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
  to a half-open UTC interval `[start, end)`. Capture entries exactly at the
  inclusive start with `kmp_goto`, then use the strictly-after `kmp_forward`
  from start; paginate, merge/deduplicate refs and exclude the end. Complete
  the traversal or report its exact continuation.
- **Ask semantically, with bounded language fallback.** For a non-temporal
  question, make one initial Ask selection per language: once in the user's
  language, then at most once per fallback language configured by `kmp-mcp
  config`. Changing budget, detail or optional arguments does not authorize
  another selection in the same language. Only following
  `projection.page.next_cursor` with every bound argument unchanged is a
  continuation, not a retry. Translate only the query, answer in the user's
  language, and preserve stored evidence, refs, relation `why` and source
  metadata byte-for-byte. A genuinely semantic `UNKNOWN` after those bounded
  selections is terminal: do not inspect the about/root, widen scope or
  traverse the graph to bypass it.
- **Abouts are opaque routing identifiers.** Copy an about supplied by the user
  or returned by KMP byte-for-byte into every `about` argument. Never strip or
  add a kind prefix such as `project:` or `incident:`, and never translate,
  normalize, shorten, infer or rebuild it.
- **Refs are opaque identifiers.** Pass every returned ref, and any exact
  stored ref supplied by the user, byte-for-byte. Never prefix or qualify it
  with an about, translate it, normalize it or reconstruct it. If a ref fails,
  recover the exact stored ref through KMP instead of guessing.
- **Let the result route the next move.** After bounded Ask retries,
  reclassify the original goal: current/latest/recent state, what changed,
  why now and release or decision history use temporal navigation; only a
  genuinely semantic unresolved question ends at `UNKNOWN`. A relevant
  recall page or temporal interval must be completed before switching to
  repository evidence. Before a consequential claim, `kmp_inspect` audits its
  cited ref; `kmp_trace` proves a claimed connection between refs.
- **Write decisions, constraints and outcomes — never transcripts.** Memory
  is the durable shape of the work, not a log of the conversation. Prefer
  `kmp_write_memory`, which validates intent and relation quality before
  canonical ingest. Normal writes commit in one call with
  `options.dry_run=false` (or the option omitted); validation failures write
  nothing.
  Use `options.dry_run=true` only for an explicitly requested preview,
  debugging the compiled payload or deliberate human review.
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
