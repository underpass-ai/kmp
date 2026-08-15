# kmp-plugin-api

The public plugin API of [KMP by Underpass](https://github.com/underpass-ai/kmp),
the Kernel Memory Protocol kernel.

Deliberately small. A plugin can depend on this crate without pulling in
kernel aggregates, ports, adapters, storage clients, gRPC, MCP or any runtime
infrastructure — which is the whole point: a plugin should compile against a
contract, not against a kernel.

## The two contracts

- **Value plugins** implement `EvidenceValuePlugin` and turn retrieved
  evidence fragments into typed mentions.
- **Derivation plugins** implement `EvidenceDerivationPlugin` and compute
  deterministic results from explicit operands.

What a plugin does not decide: storage, traversal, refs, provenance or
ranking. Those stay with the kernel. Readers and agents decide which mentions
are included, excluded or kept as context for the question at hand.

These are compile-time Rust traits — reusable crate boundaries, not a dynamic
ABI or a runtime plugin registry.

## License

Apache-2.0.
