# AsyncAPI Contracts

The first event contract lives in `context-projection.v1beta1.yaml`.

It documents:

- inbound node-centric projection events consumed by the kernel;
- the shared event envelope required across subjects;
- outbound notifications emitted after bundle generation.

This AsyncAPI contract is the generic kernel-owned async boundary.

The contract gate validates that this boundary stays free of product-specific
legacy nouns and remains aligned with the descriptor tests in
`kmp-proto`.

Reference event fixtures live under:

- [`api/examples/kernel/v1beta1/async`](../examples/kernel/v1beta1/async)
