Show me the KMP memory and ChronoLoom moves and when to use each one.

Prefer the live surface: if the `kmp` MCP server is available in
this session, describe what it actually exposes, including the relation
vocabulary carried on `connect_to.rel`. That catalog is generated from the
kernel's own writer spec and is the authority on relation types. Fall back to
the map below only if the server is unreachable, and say that you are doing
so.

**Entry**
- `kmp_wake {about}` — compact packet: state, decisions, open threads.
  Call it before re-deriving context by reading files.
- `kmp_ask` — deterministic evidence answer for a semantic question, or
  `UNKNOWN`. Never generated. Ask in the user's language first; only semantic
  Ask may retry a translated query from the configured bounded list.

**Time** — each takes a timestamp, a sequence number, or a ref
- `kmp_goto` — the state at a point (defaults to 50 entries)
- `kmp_near` — the neighborhood around a point
- `kmp_rewind` — how we got here
- `kmp_forward` — what happened next

Temporal intent has precedence over Ask. For yesterday/today, since, before,
after, during, dates or release windows, resolve the user's timezone to an
explicit half-open UTC interval `[start, end)`, navigate time first, and follow
every continuation cursor until the interval is complete. Since `kmp_forward`
is strictly after its cursor, capture the inclusive start with `kmp_goto`, then
move forward from start, merge/deduplicate refs and exclude the end.

**Audit**
- `kmp_trace` — the proof path between two refs
- `kmp_inspect` — one ref: stored object, links, evidence

**Write**
- `kmp_write_memory` — the default. Validates intent and relation quality,
  then commits through canonical ingest. Normal writes use one call with
  `options.dry_run=false` or the option omitted; invalid writes write nothing.
  `options.dry_run=true` is only for an explicit preview, payload debugging or
  deliberate human review.
- `kmp_ingest` — canonical low-level form.

Close with the two rules that matter: **write decisions, constraints and
outcomes — never transcripts**, and for a rich relation **`why` explains the
specific semantic link while `evidence` is the concrete observation or source
that proves the rationale**. KMP uses both in recall and audit but generates
neither. Point writers to “Why the `why` matters” in the `kmp-memory` skill;
a vague `related_to` is a bug rather than a shortcut.

For language fallback, make one initial Ask selection per language: once in
the user's language, then at most once in each configured fallback language.
Changing budget, detail or optional arguments does not authorize another
selection in the same language. Only following
`projection.page.next_cursor` with every bound argument unchanged is a
continuation, not a retry. Translate only the query and answer in the user's
language. Stored evidence, refs, relation `why`, and source metadata stay
byte-for-byte unchanged. A genuinely semantic `UNKNOWN` after those bounded
selections is terminal: do not inspect the about/root, widen scope or traverse
the graph to bypass it.

Abouts are opaque routing identifiers. Copy an about supplied by the user or
returned by KMP byte-for-byte into every `about` argument. Never strip or add a
kind prefix such as `project:` or `incident:`, and never translate, normalize,
shorten, infer or rebuild it.
Refs are opaque identifiers: pass every returned ref, and any exact stored ref
supplied by the user, byte-for-byte. Never prefix or qualify it with an about,
translate it, normalize it or reconstruct it. If a ref fails, recover the exact
stored ref through KMP instead of guessing.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
