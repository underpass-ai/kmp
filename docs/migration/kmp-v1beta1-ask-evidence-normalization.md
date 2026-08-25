# KMP v1beta1 Ask evidence normalization

`kmp_ask` now serializes each complete evidence body once, in
`proof.evidence[]`. The answer, cited reasons, and proof hops join that registry
by stable evidence id instead of copying the body into every layer.

## Contract changes

- `answer` is a deterministic citation-oriented index. It names the supported
  claim refs and evidence ids and tells the consumer that canonical text is in
  `proof.evidence`.
- `because[].ref` is the canonical `proof.evidence[].id`; `because[].claim` is
  the supported claim ref.
- `AnswerReason.evidence` remains protobuf field 2 for v1beta1 wire
  compatibility, but new recall responses leave it empty. The MCP JSON binding
  omits empty compatibility fields.
- `MemoryRelation.evidence_refs` is additive protobuf field 10. Proof hops list
  the canonical evidence ids that support them. A relation-specific `why` or
  `evidence` remains inline unless it exactly repeats a canonical evidence
  body; exact repetitions are removed.

The JSON Schema now makes `because[].evidence` optional and permits
`relation.evidence_refs` on relation output. Ingest still requires a
non-structural relation to carry its own `why` or `evidence`; a ref alone does
not satisfy that write contract. This is a v1beta1 semantic migration, not a
new protocol version: old protobuf clients ignore the additive relation field,
while consumers that read `AnswerReason.evidence` must switch to the id join.

## Consumer migration

1. Build an evidence map keyed by `proof.evidence[].id`.
2. Resolve each `because[].ref` through that map.
3. Resolve each `proof.path[].evidence_refs[]` through the same map.
4. Treat inline relation `why` and `evidence` as relation-specific explanation,
   not another copy of the evidence registry.
5. Use `kmp_inspect` with a cited ref when the inline registry was omitted by
   an output budget.

This preserves provenance, confidence, conflicts, supersession, and stable
refs while reducing serialized size and allocation pressure. Stable pageable
projection and normative byte ceilings remain the scope of issue #92.

## Frozen-fixture result

The three-reason regression serializes each complete body exactly once and
checks three byte-identical runs. On the development profile it reduced the
canonical packet from 25,366 to 6,440 bytes (74.6%) and the advisory cl100k
estimate from 4,096 to 1,319 tokens. Owned evidence-body bytes, used as the
allocation-pressure proxy, fell from 22,554 to 3,222 (85.7%). A 250-sample
Serde microbenchmark is printed by the test to expose serialization work; its
timing is diagnostic rather than a platform-independent assertion.
