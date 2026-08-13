# Kernel Contract Reference Fixtures

These fixtures are the canonical examples for the generic kernel-owned
integration boundary.

They are split by transport and version:

- `kernel/v1beta1/grpc`: ProtoJSON examples for the canonical
  `underpass.rehydration.kernel.v1beta1` gRPC contract
- `kernel/v1beta1/async`: JSON examples for the generic AsyncAPI subjects,
  plus the schema-constrained `vLLM` graph batch fixture and a stable
  incident-style GraphBatch smoke fixture
- `kernel/v1beta1/kmp`: transport-neutral Kernel Memory Protocol draft
  fixtures for `ingest`, `wake`, `ask`, temporal movement, `trace`, and
  `inspect`
- `runtime-reference/v1`: consumer-side runtime examples used by the agentic
  integration reference

They are validated by `kmp-proto` contract tests and the contract gate:

```bash
bash scripts/ci/contract-gate.sh
```

These fixtures must remain node-centric. Do not introduce product-specific
nouns such as `case_id`, `story_id`, `planning.*`, or `orchestration.*`.

The `runtime-reference/v1` folder is different:

- it is not a kernel-owned public transport contract
- it documents a recommended runtime shape for external consumers
- it should stay generic and agentic, but it is guidance rather than a
  public contract promise
