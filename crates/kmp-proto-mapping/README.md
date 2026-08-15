# kmp-proto-mapping

The mapping between the `v1beta1` proto contract of
[KMP by Underpass](https://github.com/underpass-ai/kmp) and the kernel's
application and domain types.

Transport-neutral on purpose. It was extracted from the gRPC transport so
that any composition — the gRPC server, the embedded MCP backend, a test
harness — speaks the same wire shapes without linking transport
infrastructure it does not need.

Which also means there is one place where the contract meets the model. When
a field's meaning changes, it changes here, once, for every caller.

## License

Apache-2.0.
