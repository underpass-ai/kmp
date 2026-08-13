# Runtime Reference Examples

These examples describe a reference runtime shape for consumers that use
`kmp` as a context engine.

They are intentionally separate from the kernel-owned contract examples under:

- `kernel/v1beta1/grpc`
- `kernel/v1beta1/async`

This folder is consumer-side guidance, not a formal kernel API commitment.

The shape is:

1. `POST /v1/sessions`
2. `GET /v1/sessions/{session_id}/tools`
3. `POST /v1/sessions/{session_id}/tools/{tool_name}/invoke`

The shared reference implementation lives in:

- [`runtime_http_client.rs`](../../../../crates/kmp-transport-grpc/src/agentic_reference/runtime_http_client.rs)
- [`main.rs`](../../../../crates/kmp-transport-grpc/src/bin/runtime_reference_client/main.rs)

The fake server used by the E2E lives in:

- [`fake_underpass.rs`](../../../../crates/kmp-tests-shared/src/runtime/fake_underpass.rs)
