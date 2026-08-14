Show me the KMP tool surface — the ten moves and when to use each.

Prefer the live surface: if the `kernel-memory` MCP server is available in
this session, describe what it actually exposes, including the relation
vocabulary carried on `connect_to.rel`. That catalog is generated from the
kernel's own writer spec and is the authority on relation types. Fall back to
the map below only if the server is unreachable, and say that you are doing
so.

**Entry**
- `kernel_wake {about}` — compact packet: state, decisions, open threads.
  Call it before re-deriving context by reading files.
- `kernel_ask` — deterministic evidence answer, or `UNKNOWN`. Never generated.

**Time** — each takes a timestamp, a sequence number, or a ref
- `kernel_goto` — the state at a point (defaults to 50 entries)
- `kernel_near` — the neighborhood around a point
- `kernel_rewind` — how we got here
- `kernel_forward` — what happened next

**Audit**
- `kernel_trace` — the proof path between two refs
- `kernel_inspect` — one ref: stored object, links, evidence

**Write**
- `kernel_write_memory` — the default. Validates intent and relation quality,
  then compiles to canonical ingest. Supports `options.dry_run`.
- `kernel_ingest` — canonical low-level form.

Close with the two rules that matter: **write decisions, constraints and
outcomes — never transcripts**, and **rich relations carry both `why` and
`evidence`**, so a vague `related_to` is a bug rather than a shortcut.
