# Consolidation development controls — 14 September 2026

This is a hand-authored development control for #540, not a held-out benchmark.
Sixteen preserved originals repeat eight statements about homonyms, September
and October ownership, copy verification versus publication permission, a
tentative plan, negation and an unresolved conflict. Declarations are fixed from
source prose before reader questions; no proposal-generation model runs inside
or outside the kernel in this control. Native behavioral tests separately cover
CAS, restart, rejection, source/relation mutation and all four clocks.

## Method

`scripts/consolidation/evaluate.py` creates an isolated native store, captures
source stamps, writes and repeats a view, projects three budgets and expands the
immutable audit. It also retains a deliberately rejected nonliteral quote.
The four arms use identical originals and ordered whole-record/group admission:

- source-only: original records;
- formed: originals plus a separate literal statement per source;
- deduplicated: the existing native `context project` applied to formed records;
- consolidated: the optional native versioned view.

The full formed JSON envelope sets the common byte ceilings: 13,791, 10,343 and
6,895 bytes. All four long-source arms fit. Tokens use `tiktoken 0.14.0`,
`o200k_base`; tokens and bytes are separate measurements.

Each arm/budget has one independent reader with a fresh context and identical
inherited model/settings, without overrides. Readers see only their frozen
packet and the same eight questions. They cannot read originals elsewhere,
expand refs, consult other answers or share context. The instructions require
exact visible refs, scope/negation/uncertainty preservation and abstention when
evidence is absent; repetition alone cannot establish independence. A separate
auditor checks original prose, visible citations and required fact recovery.
Nine unchanged packet/answer pairs are reused byte-for-byte after the writer
correction; the three changed consolidated packets receive new blind reads.

## Reader results

Each cell shows final-context tokens / completely recovered answers out of 8.
Correct Q7/Q8 abstentions count as complete: the sources provide neither Amber
production permission nor an exact R8 signature date. A correct abstention caused
by omitted answerable evidence is grounded but does not count as recovery.

| Arm | 100% | 75% | 50% |
| --- | ---: | ---: | ---: |
| Source-only | 2,343 / 8 | 2,053 / 7 | 1,338 / 6 |
| Formed | 2,963 / 8 | 2,135 / 7 | 1,463 / 6 |
| Presentation-deduplicated | 3,533 / 8 | 2,617 / 7 | 1,858 / 5 |
| Consolidated | 1,032 / 8 | 1,032 / 8 | 1,032 / 8 |

The corrected control has 96/96 packet-supported and original-supported answers,
86/96 complete recoveries, four partial recoveries, six appropriate abstentions
on omitted evidence, and 24/24 expected abstentions. There are zero invalid
citations or observed false entity/claim merges. Missing packet evidence causes
one temporal, eight negative/conflict and three copy-only recovery omissions;
these categories overlap. No visible qualifier was dropped by a reader.

The lost answerable material concerns October ownership at deduplicated 50%,
copy-only verification at source-only/formed 50%, and the unresolved denial or
both R8 statements in several reduced-budget baseline packets. Omission is
reported as missing evidence, not interpreted as falsity. Readers were not
allowed expansion in this comparison; native exact audit expansion is exercised
separately and original-source retrieval remains available.

## Retained failures

The first completed writer fixture strengthened Nora's “for staging” to
“staging only”. Deterministic quote validation admitted it: the source quote was
literal, but the interpretation was too strong. Five consolidated reader answers
then repeated that overstatement. Initial packet support was 96/96 while strict
original-text support was only 91/96. This is a writer error, not a reader
hallucination or proof that grouping verifies meaning. The source prose and
questions were retained unchanged; only that declaration was corrected.

The initial seed harness failure, that failed writer run, the first short-source
budget assertion failure, deliberate nonliteral-quote rejection and subsequent
corrected runs remain in separate local artifact directories. Rejected writes
commit nothing; their request/error are retained rather than removed from costs.
These controls do not estimate a general model generation-failure rate.

## Short-source counterexample

Removing only the repeated archival filler leaves all sixteen original facts in
551 tokens / 2,061 bytes, within all three budgets (4,447 / 3,335 / 2,223 bytes).
The consolidated full view still needs 1,032 tokens / 3,945 bytes. At 75% it
admits six of eight claims (798 tokens); at 50% it admits four (566 tokens).
This is a native size/admission control without another reader-quality run.

The existing lossless projection cannot fit its original-read omission manifest
under those short-source budgets. It returns 5,675 bytes with an explicit warning
at each budget; all three are recorded as failed budget deliveries, not wins.
Source-only operation is preferable for this short fixture. Neither a target
compression ratio nor lower total cost is guaranteed by enabling consolidation.

## Costs and reproducibility

Whole CLI request, response, stderr, tokenizer counts, exit codes, process-start
latencies and binary/script hashes are written to `measurements.json`. Preparation,
the accepted write, its retry, rejection, projection and immutable audit are
separate rows. A compact write receipt avoids echoing the full source audit;
source capture omits duplicate payload-text properties while retaining body,
provenance, status, clocks, relations and the source stamp.

Final long-source CLI costs (request / response / stderr tokens) are:

| Operation | Tokens |
| --- | ---: |
| Capture sources | 221 / 10,194 / 0 |
| Accept view | 2,492 / 79 / 0 |
| Idempotent retry | 2,492 / 79 / 0 |
| Reject nonliteral quote | 978 / 0 / 17 |
| Project, each budget | 20 / 1,032 / 0 |
| Exact immutable audit | 18 / 18,250 / 0 |
| Existing deduplication, 100% | 4,530 / 3,533 / 0 |
| Existing deduplication, 75% | 4,530 / 2,617 / 0 |
| Existing deduplication, 50% | 4,530 / 1,858 / 0 |

Capture + accepted write + one projection totals 14,038 transport tokens before
the reader; including retry, rejection and a full audit makes that 35,872.
The entire experiment's CLI traffic (all budgets and native arms) totals 59,574.
These counts are a cost warning against assuming one-shot savings. Identical
requests can tokenize differently after recapture because stamps contain hashes.

Each reader receives 107 question tokens in addition to the context above.
Saved JSON answer sizes at 100% / 75% / 50% are source-only 893 / 785 / 616,
formed 906 / 813 / 746, deduplicated 1,243 / 766 / 656, and consolidated
790 / 748 / 787 tokens. Context + questions + saved answers across all twelve
readers totals 34,432 visible tokens. This does not include hidden reasoning,
reader instructions, tool framing, model cache accounting or provider-billed
tokens. Do not add CLI responses and reader inputs as if both were separate model
billings. Provider-billed end-to-end cost and peak RAM
are unavailable. Manual declaration/formation effort is also unpriced. Therefore
this control cannot establish a total-cost improvement, even where reader context
shrinks. CLI timings include process startup and are not warmed-service latency.

The measured binary SHA-256 is
`5d52cc6d8ef070d8e82c80e01bf5c8e90685aa9873a56acbf4012985eb2be319`.
The final binary captures structural membership clocks as dependencies; all twelve
reader packet hashes remain equal to the audited corrected run, so those reads
and the strict audit are reused without resampling. Full source audits and native
costs were recaptured with this binary. Local final runs are
`consolidation-540-final` and `consolidation-540-short-final`.

Reproduce native artifacts with a built candidate; use a fresh output directory
for every attempt so failures cannot be overwritten:

```bash
uv run --with tiktoken==0.14.0 python scripts/consolidation/evaluate.py \
  --binary target/debug/kmp-mcp --out artifacts/consolidation-control
uv run --with tiktoken==0.14.0 python scripts/consolidation/evaluate.py \
  --binary target/debug/kmp-mcp --archive-repeats 0 \
  --out artifacts/consolidation-short-sources
```

Raw runs, reader answers, hashes and audits are retained under
`artifacts/consolidation-540-*`, which the repository intentionally excludes from
Git. This document and the native harness are versioned; the raw artifacts remain
local evidence. Broader independent writer evaluation on held-out histories is
still required before claiming general semantic quality or cost gains.
