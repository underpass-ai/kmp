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
(`valvula≈valve 0.51`). Either way, evidence, refs, relation `why` and source
metadata stay exactly as stored, and the agent answers in the user's language.

The table translates; it does not stand in for a word inside one language.
Its vectors come from a multilingual embedding, so it also puts neighbours of
one language close together (`deadline≈final`, `prefix≈prefer`). A bridged
pair therefore counts as stating what the question asked only when the
question and the memory are written in two different languages, each read
from its own function words; a memory in the question's language, or one
whose language cannot be read, still has to share the reader's words. Inside
one language the table still helps ranking, and a memory it reached arrives in
`proof.evidence` marked with its pairs, but the table never makes it an
answer. `high` is read from the reader's own words only: an anchored answer
that stands because the table translated a concept is `medium` at most, and so
is one the table alone carried to the bar. With no table installed nothing of
this applies and `kmp_ask` answers as it always did.

## The cross-language judge

The table's translation can be wrong, and it reaches more than it can
answer. A store that opted into TypeSafe Jev can have it judge both, with
`ask-cross-language.json` (`{"veto": true, "rescue": true}`; each effect is
off unless named, and without the file or without `typesafe.json` the answer
is the deterministic one):

- **veto**: an answer that stands only through the table's translation is
  asked about, one passage at a time ("does it state the answer? It may be
  written in another language"); a citation Jev finds at least `veto_at`
  (0.5) likely not to state it leaves, and the answer is what it is without
  it — usually UNKNOWN;
- **rescue**: an UNKNOWN to a question with no required anchor, for which the
  table reached memories written in the other language, is asked about up to
  eight of them and whether they answer the question at all; the likeliest
  is cited only when both judgements reach `rescue_at` (0.9). It is the
  stored text, marked `judged_by`, never `high`, and the summary says it
  rests on Jev's judgement. An anchored question is never rescued.

Measured on 2026-11-21 with `jev-1.14.0` and the shipped table, synthetic
data only (synth-v1 mono 1000, seeds 7 11 13 17 19 23), three fresh samples,
counts that held in at least two of them:

| | Off | Veto | Veto + rescue |
|:--|--:|--:|--:|
| English question, Spanish memory: answered right (of 36) | 11 | 11 | 30 (19 rescued) |
| Spanish question, English memory: answered right (of 36) | 16 | 16 | 30 (14 rescued) |
| Zero-overlap paraphrase, same language (of 72) | 0 | 0 | 0 |
| Same questions, answer removed from the store (of 144) | 2 | 0 | 0 |
| Judged negatives of the retrieval collection (of 11) | 0 | 0 | 0 |
| Anchored stress negatives (of 360) | 0 | 0 | 0 |
| Their Spanish renderings (of 264) | 6 | 2 | 2 |
| Their bare keywords, with a stray `de` / `la` (of 360 each) | 20 / 20 | 13 / 11 | 13 / 11 |

No right answer became UNKNOWN, and no rescue cited a wrong memory, in any
sample. What is left: the keyword answers that read `high` (8 in each
keyword set) come from a literal `status` and are the same with no table at
all, so the judge never sees them; the others are citations Jev also reads
as stating the answer (it judges a memory that names both identifiers and
the topic as stating it). A paraphrase in the same language is outside the
judge's reach: the table reaches nothing across a language there.
About 7 % of these asks reach Jev (140 of 2,003), at about 1,450 input tokens
each and a median of 0.15 s of Jev time (0.3 s added to the ask).

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

The language a question and a memory lean in is read from Spanish and English
function words only, from as few as one. A question or a memory in another
language leans in neither, so the table translates nothing for it; and a
single stray function word (`la`, `de`, `ha`) can make an otherwise keyword
question lean Spanish.

Questions in Chinese, Japanese or Thai are not segmented by word yet. Their
stored memory remains byte-exact and inspectable; word-based semantic
retrieval in those scripts is not supported.

Temporal requests such as "yesterday" use temporal navigation, not semantic
Ask. A semantic question that carries a date or a range is one Ask that
stands where it was asked: `as_of` for an instant, `interval` for a half-open
span, `axis` for the clock, and the proof declares where it stood.

See [retrieval defaults and store configuration](configuration.md) for the
optional integrations this page mentions.
