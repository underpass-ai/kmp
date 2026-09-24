# Optional remote evidence re-ranking with TypeSafe Jev — design

Status: approved design, not implemented. Branch `feat/jev-rerank`, cut from
`main` 943b9fbe (v0.20.0).

## Goal

Let `kmp_ask` surface admitted evidence that answers a question in other words
(the paraphrase gap, #469) by asking a remote judgement model, TypeSafe Jev,
whether each shortlisted passage answers the question. Jev is one more
ranking channel. It never writes the answer.

Success is measured, not assumed: on the judged retrieval cases
(`crates/kmp-testkit/judged/retrieval_cases.json`), including the
`paraphrase-gap` case, the ordering with re-ranking is compared with the
ordering without it against `retrieval-baseline.tsv`. The comparison is done
with a real key before the feature is described as an improvement.

`agent-token-optimization.md` lists "JEV or any remote evaluator" as out of
scope. That exclusion applied to the #544 token track only; this design is a
separate, opt-in feature.

## Decisions

| Decision | Choice |
| --- | --- |
| Use in KMP | Re-ranking of `kmp_ask` proof candidates |
| Data leaving the machine | Opt-in per store: a `rerank.json` file beside the store. Without it nothing is sent |
| Authority | RRF channel only. The five-item deterministic core, `because` and confidence stay lexical |
| Model | Pinned version (`jev-1.13.0`), not `jev-latest`, so orderings are reproducible |
| Pool | At most 40 candidates per Ask |
| New dependencies | None. The existing workspace `reqwest` (rustls) is reused |

## Provider contract

`POST https://api.typesafe.ai/v1/systemone`, `Authorization: Bearer <key>`.
The request carries `state`, `model` and a map of named `questions`. The
response carries `model`, one answer per question under the same key, and
`usage`. A `noul` answer holds the probability of "yes" in [0, 1]. Pricing is
per input token. Limits are 1,200 requests per minute and 64k tokens per
request, of which 32k covers `state` plus the longest question. A `429` may
carry `retry-after`.

## Components

All in `kmp-mcp`, following the local semantic retriever
(`semantic_candidate_provider.rs`, `loopback_semantic_retriever.rs`,
`semantic_retriever_config.rs`). There is one primary type per file.

- **Port** `serving/ports/evidence_reranker.rs`: `EvidenceReranker`. It takes
  the question, a `RerankPool` and a continuation flag, and returns
  `Result<RerankOutcome, String>` through a boxed `Send` future, like
  `SemanticCandidateProvider`. It proposes identities only; the mapping owns
  admission and evidence.
- **Adapter** `serving/adapters/typesafe_reranker.rs`: `TypeSafeReranker`. It
  makes one request per fresh selection. `state` is the question, and each
  pool entry becomes a `noul` question keyed `c<index>` with structured
  instructions `{ "passage": <text>, "question": "Does \`passage\` answer the
  question in the state?" }`. The ranking is by `noul` descending, with ties
  broken by entry ref.
- **Config** `serving/adapters/typesafe_reranker_config.rs`: it reads
  `rerank.json` beside the store, at most 8 KiB, with `deny_unknown_fields`:

  ```json
  { "provider": "typesafe", "endpoint": "https://api.typesafe.ai/v1/systemone",
    "model": "jev-1.13.0", "pool_size": 40, "timeout_ms": 20000 }
  ```

  The endpoint must be HTTPS on host `api.typesafe.ai`. `model` must be a
  pinned version and is rejected if it ends in `-latest`. `pool_size` is
  between 1 and 40 and `timeout_ms` between 1000 and 60000. **The key is
  never in the file:** it is read from `TYPESAFE_API_KEY`. A missing file
  means `Ok(None)` (off). A file without the variable, or with an invalid
  config, leaves ordinary retrieval running and adds the warning
  `evidence rerank disabled: …`.
- **Mapping** (`kmp-proto-mapping`):
  - `RerankPool` / `RerankSource` (entry ref, text, text SHA-256), built by
    `AskRetrievalContext::rerank_pool`.
  - `RerankCandidateRanking`, resolved against live admitted evidence like
    `SemanticCandidateRanking`.
  - `AskRetrievalContext::with_rerank_ranking`.

## Data flow

1. `EmbeddedAskTool::call` runs the kernel Ask, then the optional semantic
   step as today.
2. If a reranker is configured,
   `retrieval.rerank_pool(question, policy, temporal, pool_size)` builds the
   pool synchronously from the same admission as `semantic_sources`: live and
   in-scope entries only.
   - The pool is ordered as the ranker orders it.
   - Admitted items the ranker drops, meaning those with no lexical,
     relation or bridge link, fill the remaining slots in kernel bundle
     order. Without this step a pure paraphrase could never reach the model.
   - Each text is cut to 2,000 characters so the request fits the provider's
     32k budget for state plus longest question.
3. The adapter returns an ordered list of `(entry_ref, text_sha256)`.
   `with_rerank_ranking` stores it.
4. `ask_response_from_result` resolves the ranking against the live candidate
   evidence, so no entry escapes scope, time, lifecycle or content version. It
   then passes the result to `fuse_evidence` as another supplemental channel.
   - The core is untouched.
   - An item that reaches proof only through this channel carries
     `reached_by=rerank` and `rerank_model=<model>`, so it cannot enter
     `because`, raise confidence or remove UNKNOWN.
5. The response reports the remote call. A warning line names the provider,
   the model and the number of passages sent, the same way the semantic
   channel reports its resolution. It never contains passage text or the key.

**Continuation stability:** as in the loopback retriever, a selection is
frozen per fingerprint (the question, the pool's refs and hashes, and the
model), with up to 64 selections kept. Continuation pages reuse it without
calling the provider. A changed snapshot or an evicted selection requires a
fresh Ask.

## Errors and limits

- The following discard the channel with a warning and leave today's ordering:
  - 401/403.
  - 429 after at most two retries that honour `retry-after`, capped at 5 s.
  - Timeout or 5xx.
  - A response whose answer keys differ from the keys sent, a missing or
    non-finite `noul`, a value outside [0, 1], or a `model` other than the
    configured one.

  Ask itself never fails because of the reranker.
- Requests are capped at 256 KiB and responses at 256 KiB. There are no
  redirects and no proxy.
- The key never appears in logs, warnings, errors or `Debug` output. The
  config type stores no key, and the adapter's `Debug` is written by hand.

## Testing

- Unit tests:
  - Config validation: host, scheme, pinned model, bounds, unknown fields,
    and a missing key.
  - Request building: keys, truncation and structured instructions.
  - Response validation: every rejection listed above.
  - Deterministic tie-break.
- Mapping tests:
  - The pool respects scope, time and lifecycle, as in
    `semantic_candidates_cannot_escape_…`.
  - A rescued item lands after the core with `reached_by=rerank`.
  - The core, `because` and confidence are identical with and without the
    channel.
- Adapter tests against a local HTTP fixture: 200, 401, 429 with
  `retry-after`, a malformed body, a timeout, and a continuation page served
  with the fixture gone.
- Gates before pushing: `cargo fmt`, `cargo clippy`, crate tests,
  `scripts/ci/kmp-mcp-architecture-gate.sh` (600 lines, one type per file),
  and the coverage floors (≥ 80 % `kmp-mcp`).
- Operator-path verification with a real key: a store with `rerank.json`,
  `kmp_ask` over the judged cases, and orderings recorded with and without
  re-ranking. The result goes in `docs/development/evidence-rerank.md`, the
  user documentation written with the implementation.

## Out of scope

- Letting Jev reorder the core or feed `because` (option B). That is a
  separate decision, to be taken once the measurement above exists.
- Other Jev uses: relation typing, summary audit.
- The remote gRPC backend.
- Per-about allow lists.
