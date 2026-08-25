# kmp-proto

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate contains the Protobuf and
tonic bindings for its typed gRPC contract.

The wire package remains `underpass.rehydration.kernel.v1beta1` for backward
compatibility. Changing that identifier would break existing clients; it is a
transport namespace, not the current product name.

Generated at build time from the `.proto` files shipped inside this crate, with
both client and server stubs plus the file descriptor set (exported as
`FILE_DESCRIPTOR_SET`, which is what makes reflection and dynamic clients
work).

## Building this crate needs `protoc`

Code generation runs `tonic-build`, so the protobuf compiler must be on
`PATH`. Distribution packages call it `protobuf-compiler` (Debian/Ubuntu) or
`protobuf` (Homebrew); official binaries are on the
[protobuf releases page](https://github.com/protocolbuffers/protobuf/releases).

## One contract, two copies

The contract is authored in `api/proto` in the repository, where it is linted
and checked for breaking changes. This crate carries a vendored copy because a
published crate can only compile what ships inside it. The two are diffed on
every CI run, so the copy cannot quietly become a different wire.

## License

Apache-2.0.
