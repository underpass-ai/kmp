## Invoked, not assumed

KMP is opt-in. This skill governs what happens once something selects it; it
does not claim every session. Four things select it:

- the user names KMP or its memory, in any language — "usa kmp", "what does
  memory say", "check the store";
- a `/kmp:*` skill or command runs;
- the project's own instructions (`CLAUDE.md`, `AGENTS.md`) opt in;
- the MCP initialize instructions report always-on routing, which an operator
  turns on deliberately with `kmp-mcp config memory-routing always`.

Without one of those, do the work from the material in front of you and make
no KMP call. An unbidden `kmp_wake` against an empty or unrelated store is not
a free no-op: it spends a round trip and can shape the answer with evidence
nobody asked for.

Everything below is about a route already underway. Temporal precedence, page
continuation, do not leave for repository files mid-page — those govern a KMP
call in flight. None of them is a reason to start one.

## Use this as a router, not a tool glossary

Choose a lane before the first call, then let every result choose the next
move. Do not select `kmp_ask` once and keep treating the whole task as semantic
when the evidence says it is not.

Once invoked, known work enters through `kmp_wake`; apply the remaining rows
to the part of the goal the wake packet did not already answer.

| Signal in the user's goal | First move |
| --- | --- |
| Continue known work or recover its state | `kmp_wake` |
| Enumerate a period: yesterday, since, before/after, what changed, current/latest/recent state, why now, or a release/decision window | `kmp_goto`, `kmp_near`, `kmp_rewind` or `kmp_forward` |
| A semantic question that carries a date or a range: why something was decided in March, what rule held during the incident, what was known on the tenth | one `kmp_ask` with `interval` or `as_of`, and `axis` for a clock other than when it happened |
| A genuinely semantic question with no date, answerable from stored evidence | `kmp_ask` |
| What the memories of several abouts have to do with each other in a span: which facts fell in the window, how they stand in time inside the labels they share (the same dimension kind and scope), what each about declared, which contradictions still stand | `kmp_relate` |
| One cited ref must support a consequential claim | `kmp_inspect` |
| A connection between two refs is part of the claim | `kmp_trace` |
| The user asks to see, show, open, or navigate memory — including `muéstrame`, `enséñame`, `abre` or `ver` | Finish the retrieval lane, then `kmp_view_open` and `kmp_view_apply_intent` |
| A durable decision, constraint or outcome was reached | `kmp_write_memory` |
| A memory that exists is catalogued wrong, or a label was decided after it was written: put a label on it or take one off, without rewriting it | `kmp_relabel` |

`kmp_ask` is direct-evidence retrieval. It does not walk a period — the
temporal verbs enumerate — but it does stand where it is asked: `as_of` and
`interval` bound what competes and when the lifecycles are read. It does not
synthesize strategy, policy or prose. If a question asks what a campaign,
handoff or recommendation *should say*, retrieve the underlying stored
decisions in their own vocabulary and let the agent synthesize only after
retrieval. Relations may rank eligible evidence; they cannot promote unrelated
evidence into an answer.

Route again after every response:

- A missing embedded about returns `not_found`. For deliberately new work in
  the selected store, make the first write when there is a durable fact. For
  expected existing work, check the exact about and selected store. An empty
  bounded selection does not prove that the about has no memory.
- A wake or Ask projection with `has_more=true` is not exhaustive. If omitted
  material may affect the goal, follow its opaque cursor before leaving KMP;
  otherwise state that the recall was partial.
- When Ask evidence answers the question, use the returned refs. Inspect a ref
  before relying on its object or evidence for a consequential claim; trace
  the path when the claim depends on a connection.
- Abouts are opaque routing identifiers. Copy an about supplied by the user or
  returned by KMP byte-for-byte into every `about` argument. Never strip or
  add a kind prefix such as `project:` or `incident:`, and never translate,
  normalize, shorten, infer or rebuild it.
- Refs are opaque identifiers. Pass every returned ref, and any exact stored
  ref supplied by the user, byte-for-byte. Never prefix or qualify it with an
  about, translate it, normalize it or reconstruct it. If a ref fails, recover
  the exact stored ref through KMP instead of guessing.
- Ask in the kernel's search language: render the question in plain
  English, keep every number, identifier and acronym the user wrote exactly,
  and pass the user's own words as `asked_as`. If the result is `UNKNOWN` or
  the evidence does not answer, re-ask at most once in the user's own words.
  Changing budget, detail or optional arguments does not authorize another
  selection. Only following `projection.page.next_cursor` with every bound
  argument unchanged is a continuation, not a retry.
- When Ask returns `UNKNOWN` or irrelevant evidence after those two
  selections, reclassify the **original goal**. Current, latest or recent
  state, what changed, why now, and release or decision history move to
  temporal navigation. A genuinely semantic question ends at `UNKNOWN`. That
  terminal result is not permission to inspect the about/root, widen scope or
  traverse the graph to bypass Ask.
- Reclassification is not a workaround for `UNKNOWN`: Ask and temporal
  navigation answer different kinds of questions. Do not silently jump to
  repository files while a relevant KMP page or interval is incomplete. If
  files are consulted after the memory route is complete, identify them as
  repository evidence rather than stored KMP evidence.
- A temporal response with `page.has_more=true` must consume
  `page.next_cursor` until complete or report the exact continuation. Never
  present the first page as the interval.
