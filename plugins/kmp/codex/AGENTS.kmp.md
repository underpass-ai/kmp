<!-- kmp:begin -->
## Kernel memory (KMP)

Graph-temporal memory over the `kernel-memory` MCP server. Every answer is
derived from stored evidence; nothing here generates prose. `kernel_ask`
returning `UNKNOWN` is a correct result, not a failure to work around.

- **Recover before re-deriving.** On session start for known work, call
  `kernel_wake {about}` before reading files to reconstruct context. Abouts
  are stable ids: `project:<name>`, `incident:<id>`. An empty wake packet
  means there is no memory yet — start writing one.
- **Ask, then navigate, then audit.** `kernel_ask` for targeted questions;
  `kernel_goto` / `kernel_near` / `kernel_rewind` / `kernel_forward` to move
  through history by timestamp, sequence or ref; `kernel_trace` for the proof
  path between two refs; `kernel_inspect` for one ref's object, links and
  evidence.
- **Write decisions, constraints and outcomes — never transcripts.** Memory
  is the durable shape of the work, not a log of the conversation. Prefer
  `kernel_write_memory`, which validates intent and relation quality before
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
