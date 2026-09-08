## Route before retrieving: temporal intent wins

Classify the request before choosing `kmp_ask`. `yesterday`, `today`, `since`,
`before`, `after`, `during`, an explicit date or timestamp, and a release
window are temporal intent. The same applies in the user's language — for
example `ayer`, `hoy`, `desde`, `antes`, `después` and `durante`. Temporal
intent takes one of two shapes, and the shape decides the lane.

**Enumerate a period.** "What happened yesterday", "what changed since the
release", "what is current": the user wants what memory holds for a span,
in order. Enter the temporal lane first; do not spend an Ask call and do not
present Ask as an exhaustive interval query.

**Answer a semantic question that carries a date.** "Why was the launch
postponed in March", "what rule held during the incident", "what did we
know on the tenth": the user wants an answer, and the date says where to
stand. This is one `kmp_ask`, not a walk. Pass the same half-open UTC
interval as `interval` — `{ "start": "…Z", "end": "…Z" }`, either side may
be open — or the instant as `as_of` — `{ "time": "…Z" }`, or `{ "ref": "…" }`
to stand where that entry happened. Name `axis` only when the question is
about a clock other than when it happened: `observed` for what was known,
`ingested` for what had been written, `validity` for what held. The kernel
admits only what falls inside, weighs the question's words against the
span's own collection, reads supersession and expiry as they stood then —
an entry replaced or expired *after* the instant is current for that
question — and declares where it stood in `proof.interval`, `proof.as_of`
and `proof.axis`. `UNKNOWN` within an interval is one of two things, and
`proof.nearest_outside` tells them apart: when it names a ref, the memory
exists outside the span, so widen the interval on purpose; when it is null,
nothing bearing on the question was found anywhere in the neighbourhood.
`kmp_wake` takes the same three arguments to bound a resume packet.

Temporal intent does not require an explicit date. “What is current?”, “what
changed?”, “why now?”, release readiness, and recent decision history ask for
state across time. A version number alone is not enough to choose the lane:
asking for a stable contract at that version may be semantic, while asking how
the project reached it or whether it is the current state is temporal.

When current state or a release is temporal but the user gave no boundary,
read the real clock and start with `kmp_rewind` from now using
`limit: { entries: 1 }` to find the frontier. Use that ref or timestamp with
`kmp_near` / `kmp_rewind`, and continue until the relevant decision window is
covered. Do not invent a date merely to fit the bounded-interval recipe.

Resolve relative dates in the user's timezone. If the timezone is genuinely
unknown and changes the answer, ask for it. Convert a bounded calendar window
to an explicit half-open UTC interval `[start, end)`. Use `kmp_goto`,
`kmp_near`, `kmp_rewind` or `kmp_forward`, keep only entries whose effective
time is inside the interval, and continue while `page.has_more`.

The start is inclusive but `kmp_forward` is strictly after its cursor. First
call `kmp_goto` at `start` and retain entries whose effective time equals
`start`; discard older state. Then call `kmp_forward` from the same `start` for
the strictly later entries. Merge and deduplicate refs from both reads. Put
each returned `page.next_cursor` in the next move's cursor field — for example
`from.ref` for `kmp_forward` — while keeping the other arguments unchanged.
Exclude entries at or after `end`. If a budget or selection cap prevents a
complete boundary probe or interval, report the exact continuation action;
never call a partial page the whole period.

## `observed_at` is the real clock, in UTC

Every normal write carries `observed_at`: when this information was observed.
It is distinct from `occurred_at` (when the event happened), ingestion time
(when the kernel recorded it), and the validity interval (when it held).
Select the clock that answers the question; reads are not all ordered by observation.
**Read the clock; do not compose a timestamp.** Local wall-clock time with a
`Z` on the end is valid RFC3339 and the wrong instant, and it puts the entry
above the present — where `kmp_forward` from a correct "now" never finds
it, and the delta comes back empty looking exactly like a quiet week.

An observation stamp more than five minutes ahead of the kernel's clock is
refused at write time. For an incident that occurred yesterday but was first
observed this morning, keep yesterday in `occurred_at` and this morning in
`observed_at`. A backfill preserves a genuinely known earlier observation;
it does not copy event time into the knowledge clock merely because it is earlier.
