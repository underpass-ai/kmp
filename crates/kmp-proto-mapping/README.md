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

## License

Apache-2.0.
