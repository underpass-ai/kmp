# Weighing Ask's focus rule by IDF: measured, not adopted

Ask's strict policies admit a candidate into the cited core only when it
answers at least ⌈2/3⌉ of the question's focus concepts, every concept
counted alike. This is coordination level matching. On a real store it
kept the #187 decisions out of the core of "What current implementation and
review evidence exists for GitHub issue #187 auditable successor?": they
carried "187" and "successor" but only 5 of the 9 concepts.

The literature says to weigh concepts by specificity instead:

- Spärck Jones, *A statistical interpretation of term specificity and its
  application in retrieval*, 1972;
- Robertson, *Understanding inverse document frequency*, 2004;
- Robertson and Zaragoza, *The Probabilistic Relevance Framework: BM25 and
  Beyond*, 2009;
- Bendersky and Croft, *Discovering key concepts in verbose queries*, 2008.

KMP already computes each term's BM25 IDF over the question's own candidates,
so the change was tried there: a candidate must cover 2/3 of the focus
concepts' IDF weight.

## What was measured (26 Sep 2026)

`retrieval_kmp_scorecard` (70 questions) and the Jev judged corpus
(`jev_kmp_scorecard`, 16 asks):

| focus rule | recall@1 | MRR | core precision | false UNKNOWN | Ask MRR, plain | Ask MRR, Jev re-rank | unit tests |
|---|---|---|---|---|---|---|---|
| count, 2/3 of concepts (current) | 0.843 | 0.907 | 0.886 | 0.057 | 0.19 | 0.94 | pass |
| IDF, absent concepts weigh 0 | **0.871** | **0.936** | **0.914** | **0.029** | **0.47** | 0.80 | 3 fail, 1 a safety failure |
| IDF, absent concepts weigh the most | 0.814 | 0.879 | 0.857 | 0.086 | 0.19 | 0.94 | 3 fail |
| IDF, absent concepts weigh the mean | 0.843 | 0.907 | 0.857 | 0.086 | 0.19 | 0.94 | 3 fail |
| IDF over present concepts + half of all concepts | 0.871 | 0.936 | 0.886 | 0.057 | 0.25 | 0.94 | 5 fail, 2 safety failures |

In the "absent concepts weigh 0" row, the unit-test failures come from an
earlier variant of it that also admitted a candidate when none of the
question's concepts was present.

## Why it was not adopted

The gain comes from not penalising question words that no candidate carries.
That is exactly what the counted rule's safety tests guard against:

- **Answering from context when the subject is missing.** "Which database
  engine was used when the CI workflow concluded…?" was answered by a fact
  about the workflow alone.
- **Partial overlaps.** "Was the kernel prefix a deliberate decision?" was
  answered by "The kernel prefer mode delivered a decision…".

Weighing the absent concepts back in, at the maximum or the mean IDF,
restores safety but loses the whole gain.

With Jev re-ranking on, every weighted variant also lost two
paraphrase-trap questions. It admits a fuller lexical core, and the fusion
keeps the core ahead of the judge's order.

## What helped instead

- **#875** searches a ref's slug, not its address. The address gave every
  `success_path` row "evidence" and "current" for free. On the real
  question it removed four unrelated entries from the core.
- **Identifier-led questions** ("issue 187 successor reservation") already
  cite the #187 decisions.

## If this is revisited

The open problem is telling a question word the store phrases differently
(paraphrase, which should not count against a candidate) from a subject the
store does not hold (which must). IDF cannot tell them apart. The lexical
bridge, restatements or a judge reading the question can. Any new rule must
keep `strict_policies_reject_context_that_omits_the_requested_subject` and
`shared_prefixes_cannot_turn_unrelated_evidence_into_an_answer` passing.
