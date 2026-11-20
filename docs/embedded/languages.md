# Languages: search in one, keep the evidence in all

KMP never translates or rewrites stored evidence. This page is the detail
behind the one-line promise in the README: ask in any language, cite the
stored text byte for byte, answer in the user's language.

## How a question is asked

A semantic question is asked in the kernel's search language: the agent
renders it in plain English, keeps every number, identifier and acronym the
user wrote, and passes the user's own words as `asked_as`. The kernel searches
the rendering as given, echoes `asked_as` on the answer, and warns when the
rendering dropped an identifier or leans to another language. It accepts a
question in any language, so if the English one returns `UNKNOWN` the agent
re-asks once in the user's own words and stops.

## Crossing a language inside the kernel

With a lexical-bridge table installed — `kmp-mcp setup` installs the one the
release publishes, once for the machine, and `scripts/lexical-bridge/` builds
your own — `kmp_ask` also reaches memory written in another language on its
own: a citation that crossed a language names the word pairs that carried it
(`valvula≈valve 0.51`) and answers at medium confidence at most. Either way,
evidence, refs, relation `why` and source metadata stay exactly as stored,
and the agent answers in the user's language.

## English search summaries

A writer can attach an English rendering of a memory as the reserved entry
metadata key `summary_en`. `kmp_ask` searches it and never cites it: a
question in English reaches a memory written in Spanish through the summary,
and what is cited is the Spanish text byte for byte. The kernel lints the
summary rather than trusting it — `kmp_ingest` warns about one that leans to
another language, is too thin, repeats the text, or drops an identifier the
text carries, and ranking makes the same reading, so such a summary carries
nothing. A citation the summary carried says so: `matched_via: summary`, with
the question's words the rendering supplied in `summary_terms`.
`kmp_write_memory` takes it as `memories[].summary_en`, and a strict write
requires it when the memory is not written in English. A memory written
before summaries existed still owes one: `kmp-mcp summaries pending` lists
them, the doctor counts them, and the agent attaches each with
`kmp_write_memory` with `search_summaries` containing `ref` and `summary_en`,
the stored text untouched.

## Limits

Questions in Chinese, Japanese or Thai are not segmented by word yet. Their
stored memory remains byte-exact and inspectable; word-based semantic
retrieval in those scripts is not supported.

Temporal requests such as "yesterday" use temporal navigation, not semantic
Ask. A semantic question that carries a date or a range is one Ask that
stands where it was asked: `as_of` for an instant, `interval` for a half-open
span, `axis` for the clock, and the proof declares where it stood.

See [retrieval defaults and store configuration](configuration.md) for the
optional integrations this page mentions.
