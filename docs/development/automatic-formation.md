# Optional local memory formation

Experimental writer in `scripts/formation/`. It runs outside the deterministic
kernel and produces a canonical `kmp_ingest` payload. It does not run on ordinary
writes, alter a personal store, or claim that the automatic-formation gap is closed.

The writer receives a bounded source episode, extracts atomic memories with a
local model, checks draft citations, revises source coverage once, checks the
revised citations, asks the same model to verify each whole claim against the
complete episode, and compiles accepted memories with their
unchanged source records and `derived_from` relations. Rejected candidates remain
in the plan. A model error, incomplete verdict list or truncated response prevents
plan generation. An empty draft still receives a coverage review; an empty final
review is valid and keeps the original sources without generated memories.

## Run

Use a local server with chat-completions JSON-schema output and `/tokenize`, such
as the existing local evaluation server. The Python code uses only the standard
library. Supply the pinned model identity from the server's deployment manifest;
the writer records that identity but cannot attest the weights loaded by a server.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 scripts/formation/form.py \
  --episode scripts/formation/example-episode.json \
  --output artifacts/formation-example \
  --model nemotron-local \
  --model-revision nvidia/NVIDIA-Nemotron-3-Nano-30B-A3B-FP8@9bee19446c0dfd01f356e10979d225b2a6621944
```

The output directory must be new. It contains the original episode, complete
model requests/responses and usage, initial draft, coverage review, final
extraction, verification, plan, canonical
ingest and completion/failure manifest. No paid endpoint is used. HTTP endpoints
must be literal loopback addresses; proxies and redirects are disabled. Full
requests contain source text, so keep these artifacts with the source's access
permissions.

Apply the saved `ingest.json` as the arguments of `kmp_ingest` to an explicitly
selected evaluation store. Replaying that exact saved payload uses one stable
idempotency key. Regenerating with the same source/model/prompt version can yield a
different payload under that key and must be treated as a conflict, not retried
with a fresh key to force acceptance. Change the pipeline version for a new
experimental formulation. This increment does not provide a store-writing CLI.

## Contract and limitations

- The input admits only `about`, `episode_id`, and source records with `id`, `role`,
  `text`, `observed_at`. Queries, answers and benchmark labels are rejected as
  unknown fields. This prevents accidental field leakage; callers must still
  exclude evaluation labels from source text and identifiers.
- Entire turns remain intact. Over 24,000 source characters or 128 sources is
  refused; oversized prompts are refused before generation. There is no silent
  truncation. The optional session planner below packs longer sessions before
  formation. The paired session compiler below deduplicates originals; context
  across sessions remains separate work.
- Generated memories retain source roles, source hashes, model revision, literal
  quotes and a rationale per derivation. Original source text remains verbatim.
  A literal quote is a mechanical check, **not proof that the claim follows**.
  Same-model verification is explicitly marked and needs independent audit.
- The occurred clock of a generated memory is the latest cited source report;
  its observed clock is formation time. No event date is inferred, and neither
  clock is presented as a model-inferred event time. Relative expressions stay
  qualified in the text. A source later than the attempt is rejected.
- There is no automatic entity merge, lifecycle supersession or consolidation.
  Advice must not become a user's decision. Negation, uncertainty, speaker and
  date fidelity require measured model tests beyond deterministic validation.
- Before general use, compare source-only ingestion versus formed memories on
  retrieval, answer accuracy, unsupported claims, coverage, citations, latency
  and tokens, keeping sources, readers and question sets fixed.

Pipeline `formation-v2` tightens the model instructions after the first source
controls: form self-contained memories from explicit antecedents, preserve
reporting anchors and temporal restrictions, and accept uncertain statements
when they faithfully retain the source's uncertainty. These are instructions to
be measured, not additional deterministic guarantees. Earlier `formation-v1`
artifacts remain a separate baseline; the version changes the logical write key.

`formation-v3-no-thinking` keeps those prompts and tests the same local model
with thinking disabled. Generation settings are recorded in the manifest and
every request. This experimental profile must earn its fidelity in the source
controls; disabling reasoning does not itself establish a quality improvement.
The six local development controls found omissions and incomplete antecedent
citations in that profile, despite fewer output tokens. It is not validated for
general formation. The earlier versioned artifacts remain separate evidence.

`formation-v4-coverage-review` adds one source-only revision after extraction,
including when the draft is empty. It may retain, remove, repair or add memories
but cannot commit them directly: literal citation admission and whole-claim
verification still apply. An invalid or truncated review fails the attempt;
there is no retry loop. A rejected final claim is retained as rejected, not
repaired again. Review notes are diagnostic and never become graph evidence.

This candidate uses the same model with thinking enabled only for the coverage
review; extraction and final verification keep the faster non-thinking profile.
The manifest records defaults and phase overrides, and each model request keeps
its actual settings and usage. There are at most three generation calls per
episode, or two when the final revision contains no admissible candidates.
These choices require measured coverage, lineage and cost controls; another
call to the same model is not independent validation or a completeness guarantee.

`formation-v5-review-sampling` changes only the reasoning review's sampling to
temperature 1.0 and top-p 1.0, with seed 0 and the same output limit. This follows
the [pinned model's reasoning guidance](https://huggingface.co/nvidia/NVIDIA-Nemotron-3-Nano-30B-A3B-FP8).
The non-thinking extraction and verification remain greedy. The twelve local
V4 controls exposed omissions and incomplete antecedent citations; this is a
controlled candidate to measure, not a claimed fix for those model failures.

Run the boundary tests without loading a model:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/formation -p 'test_*.py' -v
```

## Prepare a longer session

`segment.py` accepts exactly `about`, `session_id` and `sources`, with the same
source fields as an episode. A session has one reporting timestamp. Split
different reporting sessions explicitly; the planner never carries a later
session back into an earlier episode.

```bash
python3 scripts/formation/segment.py --session session.json --output segments.json
```

The `whole-turn-session-v1` plan packs up to 8,000 source characters and 32
sources per episode by default. It keeps a larger single turn intact up to the
writer's hard 24,000-character limit and marks it as exceeding the target.
Larger turns fail the complete planning operation. Blank turns are listed as
excluded from generation; callers retain the complete original session.
There is no source truncation or model call in planning.

When it fits, an episode includes the preceding turn from the same session.
The plan distinguishes these context sources from newly covered sources and
records every omitted overlap. Each nonblank input turn is newly covered once,
in input order and with unchanged text, identity, role and timestamp. This
single-turn overlap cannot preserve every distant antecedent. The model client
still checks token admission for every generation phase; character packing is
not a guarantee that a draft plus sources will fit.

Episode IDs bind the complete session and segmentation policy. Plans are for
preparing independent formation attempts, not for blindly concatenating their
ingests: the current episode planner scopes original-source refs per episode,
so overlapping sources require the session compiler below.
No retrieval or quality benefit is claimed from segmentation alone.

## Compile paired session stores

`session_plan.compile_session(session, attempts, **policy)` rebuilds the episode
boundaries and requires exactly one terminal outcome per episode. A completed
attempt contains `episode_id`, `status`, `version`, `model_revision`,
`observed_at`, `extracted` and `verified`. A failed attempt has the same identity,
model and clock fields, with `error` instead of version and model outputs.
Completed attempts must match the running writer version; their candidates and
verdicts are validated again before any ingest is compiled.

The result contains `baseline` and `formed` payloads for **separate stores**.
Each nonblank original appears exactly once in both variants with identical
ref, text, metadata and coordinates. Only the formed variant adds accepted
memories, quotes and derivations, translating episode source refs to the shared
session originals. Failed episodes remain in `results`, mark the plan partial,
and never remove original sources. An all-blank session returns no ingest.
Blank originals stay in the caller's saved session and are listed explicitly.

This function does not call a model or write a store. Save and replay the exact
payload; regenerating a different result with the same session, policy, writer
version and model set intentionally reuses the write key and may conflict.
Original sources are deduplicated by identity, while repeated derived claims
are retained for later measurement. This is not automatic entity resolution,
consolidation, or evidence that the generated memories improve retrieval.

Design references: [Hindsight retain](https://hindsight.vectorize.io/developer/api/retain)
describes fact extraction and timestamp/context conditioning;
[Graphiti extraction](https://github.com/getzep/graphiti/blob/main/graphiti_core/utils/maintenance/edge_operations.py)
associates extracted graph facts with source episodes. These inform the experiment;
they do not establish a comparable KMP score.
