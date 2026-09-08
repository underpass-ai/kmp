## The question is asked in English; the answer is given in the user's language

The kernel matches words and never translates. What reaches a memory written
in any language is its English `summary_en`, so a semantic question is asked
in the kernel's search language: render it in plain English, keep every
number, identifier and acronym the user wrote exactly (`v0.7.0`, `#469`,
`kmp-mcp`), and pass the user's own words as `asked_as`. The kernel searches
the rendering as given, echoes `asked_as` on the answer for the audit trail,
and warns when the rendering leans to another language or drops an
identifier the user's words carry — read the warning and keep it next time;
the question was still searched.

The kernel accepts a question in any language: one in the store's own
language reaches the stored text directly, and where a lexical-bridge table
is installed a citation that crossed a language names its word pairs in
`bridged_terms` at medium confidence at most. So if the English rendering
returns `UNKNOWN`, or the evidence does not answer, re-ask at most once in the
user's own words, then reclassify the original goal before stopping. Never
translate or rewrite stored evidence,
refs, relation `why` or source metadata: cite the stored text byte-for-byte
and write the answer in the user's language. A period to enumerate is
navigated, not asked; a semantic question with a date is asked this same
way, with `interval` or `as_of` beside the English rendering.
