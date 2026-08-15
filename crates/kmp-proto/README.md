# kmp-proto

Protobuf and tonic bindings for
`underpass.rehydration.kernel.v1beta1` — the typed gRPC contract of
[KMP by Underpass](https://github.com/underpass-ai/kmp), the Kernel Memory
Protocol kernel.

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
