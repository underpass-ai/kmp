# Optional local memory formation

Experimental writer in `scripts/formation/`. It runs outside the deterministic
kernel and produces a canonical `kmp_ingest` payload. It does not run on ordinary
writes, alter a personal store, or claim that the automatic-formation gap is closed.

The writer receives a bounded source episode, extracts atomic memories with a
local model, checks literal citations, asks the same model to verify each whole
claim against the complete episode, and compiles accepted memories with their
unchanged source records and `derived_from` relations. Rejected candidates remain
in the plan. A model error, incomplete verdict list or truncated response prevents
plan generation. An empty extraction is a valid result.

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
model requests/responses and usage, extraction, verification, plan, canonical
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
  truncation. Splitting longer histories and preserving cross-episode context
  remain future work.
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

Run the boundary tests without loading a model:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/formation -p 'test_*.py' -v
```

Design references: [Hindsight retain](https://hindsight.vectorize.io/developer/api/retain)
describes fact extraction and timestamp/context conditioning;
[Graphiti extraction](https://github.com/getzep/graphiti/blob/main/graphiti_core/utils/maintenance/edge_operations.py)
associates extracted graph facts with source episodes. These inform the experiment;
they do not establish a comparable KMP score.
