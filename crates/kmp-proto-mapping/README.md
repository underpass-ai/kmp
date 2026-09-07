# kmp-proto-mapping

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate maps its `v1beta1` proto
contract to kernel application and domain types.

Transport-neutral on purpose. It was extracted from the gRPC transport so
that any composition — the gRPC server, the embedded MCP backend, a test
harness — speaks the same wire shapes without linking transport
infrastructure it does not need.

Which also means there is one place where the contract meets the model. When
a field's meaning changes, it changes here, once, for every caller.

## Experimental semantic candidates

Rust callers may pass `AskRetrievalContext::from(result)` with a
`SemanticCandidateRanking` to `ask_response_from_result`. The optional ranking
binds a question, an immutable encoder revision and up to 100 unique entry refs
with SHA-256 fingerprints of their exact stored text. The mapping resolves only
live sources admitted by the selected context and clock; unknown refs, stale
text and expired or superseded sources cannot be rescued by similarity.

The existing five-citation core is retained. Remaining proof candidates combine
lexical and semantic ranks using reciprocal rank fusion (constant 60), with one
vote per channel. Semantic-only proof carries `reached_by=semantic` and encoder
provenance. It does not establish a citation, raise confidence or turn UNKNOWN
into an answer. Entry caps and the existing transport byte projection still
apply. Replaying a ranking for another question is rejected.

This is an experimental integration boundary on `work/sota-gaps`. The embedded MCP backend now supports an [optional loopback encoder/index
adapter](../../docs/development/semantic-retrieval.md); remote gRPC still uses
its existing retrieval. This crate makes no model or network calls. The boundary tests verify
paraphrase retrieval and isolation, not benchmark accuracy. The separate
evaluation project records the fixed local dense/BM25/fusion comparison.

## License

Apache-2.0.
