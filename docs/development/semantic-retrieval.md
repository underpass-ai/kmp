# Optional local semantic retrieval

The embedded MCP backend can supplement Ask proof with a local encoder. This
is experimental; enabling it does not establish an accuracy improvement.
The remote gRPC backend does not yet use this adapter.

Place `semantic-retrieval.json` beside the selected store and restart its MCP
process to opt in:

```json
{
  "endpoint": "http://127.0.0.1:8001/rank",
  "model_revision": "Qwen/Qwen3-Embedding-0.6B@97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"
}
```

Only literal loopback HTTP addresses are accepted. Redirects and HTTP proxies
are disabled. The client sends only the live source texts already admitted by
the application's scope and temporal selection. The adapter proposes at most
100 refs per channel and SHA-256 fingerprints, tied to the exact question and configured
model revision. KMP resolves them against its stored text and keeps source
identity, provenance and lifecycle checks. The encoder supplies no answer text.

Five deterministic citations retain priority. Other proof candidates combine
lexical and semantic ranks using RRF with constant 60. Semantic-only items carry
`reached_by=semantic` and `semantic_model_revision`; similarity cannot establish
an answer, increase confidence or remove UNKNOWN. Existing byte and entry caps
apply. Completing pages does not restore shortened core text; inspect a source
ref when its complete text is needed.

The adapter freezes up to 64 selections, including declared fallbacks, for
continuation pages. A changed source snapshot or evicted selection requires a
fresh Ask. Encoder failure preserves ordinary retrieval with a warning; a fresh
selection may retry a failed endpoint. Invalid optional configuration likewise
leaves ordinary retrieval available. Requests are capped at 16 MiB, responses
at 512 KiB and calls at 60 seconds.

## Local sidecar

`scripts/semantic/server.py` provides the local protocol implementation for
Qwen3 embeddings. It requires Python, NumPy, PyTorch with a compatible CUDA
runtime, Transformers and previously downloaded weights. It never loads remote
code or downloads weights while serving. Freeze the package/container versions
and verify the model revision before measuring.

```bash
python scripts/semantic/server.py \
  --model /path/to/verified/qwen3-embedding-0.6b \
  --model-revision Qwen/Qwen3-Embedding-0.6B@97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3 \
  --cache /path/to/disposable/vector-cache.sqlite
```

The SQLite cache stores normalized vectors keyed by encoder revision and exact
text hash. It is an accelerator, not a memory store: candidates always come
from the source list of the current request. A restart reuses vectors; edits
produce a different key; deleted, out-of-scope or expired sources cannot enter
the ranking through a cache hit. It currently scans the requested vector batch,
so large-store indexing and cache garbage collection remain future work.

Tests cover the real MCP boundary, byte-limited continuation with the sidecar
gone after its first response, temporal/scope admission and unavailable-encoder
fallback. The sidecar tests check persistent cache reuse and isolation.

## Fixed lexical/dense candidate ablation

The default sidecar policy remains `dense`. The explicit
`--ranking-policy bm25-rrf-v1` adds BM25 over the exact admitted source snapshot
and fuses its ranking with dense candidates. Parameters are fixed at k1=1.5,
b=0.75, RRF constant 60 and 100 unique refs per channel. Tokenization is Unicode
word splitting after lowercasing, without stemming or query expansion. No
zero-match lexical candidate is added. Corpus statistics are recomputed only
from the supplied snapshot; duplicate transport rows cannot add votes.

For this policy, configure the store's `model_revision` as the encoder revision
followed by `/bm25-rrf-v1`; `/health` returns the required exact value. A plain
encoder revision is rejected by a hybrid sidecar. Vector-cache keys still use
only the encoder revision and text hash, so the ablation reuses the same vectors.
Freeze both the code commit and policy-qualified revision in measurement plans.

The kernel keeps its existing citation priority and fusion with graph/lexical
proof. This policy changes only the optional adapter's candidate ranking: a
second RRF stage remains in the kernel. Its quality must be measured end to end;
neither lexical matches nor fusion scores prove an answer or remove lineage
requirements. No policy is promoted solely by an individual repaired example.

Formula references: [Lucene BM25Similarity](https://lucene.apache.org/core/9_12_1/core/org/apache/lucene/search/similarities/BM25Similarity.html)
and [Elastic RRF](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/reciprocal-rank-fusion).
k1=1.5 preserves this evaluation's earlier baseline; it is not Lucene's default.


## One native fusion with independent channels

`--ranking-policy separate-bm25-v1` retains the same encoder, BM25 parameters,
source snapshot, cache and candidate limits. Its response carries `candidates`
in dense order and `lexical_candidates` in BM25 order, with at most 100 unique
refs and matching fingerprints each. The sidecar does not fuse them. Configure
`model_revision` with the `/separate-bm25-v1` suffix; an older binary rejects
this expanded response and declares a fallback instead of silently ignoring it.

KMP validates both channels against the same admitted live text, then performs
one RRF over its ordinary ranking and the two supplemental rankings. Its five
eligible core citations keep priority. Repeated canonical evidence identities
vote once per channel. The source and literal text remain unchanged. An item
found only through the adapter still carries `reached_by=semantic`; on split
channels, `retrieval_channel=dense|bm25` identifies the supplemental object
retained by fusion. This is retrieval provenance, never proof of answer support
or searchable source content. Ordinary provenance remains when it supplied the
same evidence first. An empty dense channel can still return BM25 proof without
establishing an answer.

Legacy responses without `lexical_candidates` retain single-channel behavior.
Either malformed channel causes an explicit ordinary fallback. Both rankings
are frozen together for pagination, subject to the existing source-snapshot,
model revision, byte limits and selection lifetime. Distinct text fingerprints
for one ref remain distinct stored representations; this change does not
consolidate memories, merge entities or deduplicate a source against its derived
claim. Those need separate quality measurements.

This policy is an experimental candidate. Compare it against the same binary
with the default dense adapter, keep failures, and record coverage and answer
fidelity separately. Do not promote it just because a selected query improves.
