# Optional local semantic retrieval

The embedded MCP backend can supplement Ask proof with a local encoder. This
is experimental work on `work/sota-gaps`; no SOTA or accuracy claim follows from
enabling it. The remote gRPC backend does not yet use this adapter.

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
100 refs and SHA-256 fingerprints, tied to the exact question and configured
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
