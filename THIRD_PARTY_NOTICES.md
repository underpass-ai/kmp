# Third-Party Notices

This file calls out notable direct dependencies and assets that KMP ships or
builds against. [`Cargo.lock`](./Cargo.lock) is the authoritative resolved Rust
dependency graph; published crates declare their SPDX license metadata, while
the repository and binary distributions carry the project license text. This
summary is not an SBOM.

We are grateful to their authors and contributors.

## Runtime Dependencies

| Crate | License | Purpose |
|-------|---------|---------|
| [async-nats](https://crates.io/crates/async-nats) | Apache-2.0 | NATS client and JetStream |
| [neo4rs](https://crates.io/crates/neo4rs) | MIT | Neo4j graph database driver |
| [opentelemetry](https://crates.io/crates/opentelemetry) | Apache-2.0 | OpenTelemetry API |
| [opentelemetry-otlp](https://crates.io/crates/opentelemetry-otlp) | Apache-2.0 | OTLP trace exporter |
| [opentelemetry_sdk](https://crates.io/crates/opentelemetry_sdk) | Apache-2.0 | OpenTelemetry SDK |
| [prost](https://crates.io/crates/prost) | Apache-2.0 | Protocol Buffers |
| [rust-stemmers](https://crates.io/crates/rust-stemmers) | MIT/BSD-3-Clause | Snowball stemming for recall across word forms |
| [reqwest](https://crates.io/crates/reqwest) | MIT/Apache-2.0 | HTTP client |
| [serde](https://crates.io/crates/serde) | MIT/Apache-2.0 | Serialization framework |
| [serde_json](https://crates.io/crates/serde_json) | MIT/Apache-2.0 | JSON serialization |
| [tiktoken-rs](https://crates.io/crates/tiktoken-rs) | MIT | BPE tokenizer (cl100k_base) |
| [tokio](https://crates.io/crates/tokio) | MIT | Async runtime |
| [tokio-rustls](https://crates.io/crates/tokio-rustls) | MIT/Apache-2.0 | TLS for Tokio |
| [tonic](https://crates.io/crates/tonic) | MIT | gRPC framework |
| [tracing](https://crates.io/crates/tracing) | MIT | Structured diagnostics |
| [tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry) | MIT | OpenTelemetry bridge for tracing |
| [tracing-subscriber](https://crates.io/crates/tracing-subscriber) | MIT | Tracing output formatting |

## Build Dependencies

| Crate | License | Purpose |
|-------|---------|---------|
| [prost-build](https://crates.io/crates/prost-build) | Apache-2.0 | Protocol Buffers code generation |
| [tonic-build](https://crates.io/crates/tonic-build) | MIT | gRPC code generation |

## Vendored Web Assets

| Library | License | Purpose |
|---------|---------|---------|
| [pixi.js](https://pixijs.com) 8.19.0 | MIT | WebGL renderer for the embedded memory viewer; vendored bundle, hash-pinned and supply-chain-verified in `crates/kmp-viewer/ui/vendor/VENDOR.md` |

## Shipped Assets

| Asset | License | Purpose |
|-------|---------|---------|
| [`distribution/lexical-bridge/kmp-lexical-bridge.kmpb`](./distribution/lexical-bridge/README.md) | Apache-2.0 | The lexical-bridge table `kmp-mcp setup` installs: vectors derived from [`sentence-transformers/static-similarity-mrl-multilingual-v1`](https://huggingface.co/sentence-transformers/static-similarity-mrl-multilingual-v1) (Apache-2.0); word list drawn from that model's [`bert-base-multilingual-uncased`](https://huggingface.co/google-bert/bert-base-multilingual-uncased) tokenizer vocabulary and from the [`sentence-transformers/LaBSE`](https://huggingface.co/sentence-transformers/LaBSE) tokenizer vocabulary (both Apache-2.0, Google). Provenance, revisions and measurements are recorded beside the file. |

## Dev/Test Dependencies

| Crate | License | Purpose |
|-------|---------|---------|
| [tempfile](https://crates.io/crates/tempfile) | MIT/Apache-2.0 | Temporary files for tests |
| [testcontainers](https://crates.io/crates/testcontainers) | Apache-2.0 | Container-backed integration tests |
