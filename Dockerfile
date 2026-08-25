FROM rust:1.97.1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY api ./api
COPY crates ./crates

RUN cargo build --locked --release \
    -p kmp-server --bin kmp-server \
    -p kmp-mcp-http --bin kmp-mcp-http \
    -p kmp-transport-grpc --bin runtime_reference_client

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/kmp --shell /usr/sbin/nologin kmp

COPY --from=builder /workspace/target/release/kmp-server /usr/local/bin/kmp-server
COPY --from=builder /workspace/target/release/kmp-mcp-http /usr/local/bin/kmp-mcp-http
COPY --from=builder /workspace/target/release/runtime_reference_client /usr/local/bin/runtime-reference-client
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/doc/kmp/

ENV KMP_SERVICE_NAME=kmp \
    KMP_GRPC_BIND=0.0.0.0:50054 \
    KMP_GRAPH_URI=neo4j://neo4j:7687 \
    KMP_DETAIL_URI=redis://valkey:6379 \
    KMP_SNAPSHOT_URI=redis://valkey:6379 \
    KMP_RUNTIME_STATE_URI=redis://valkey:6379 \
    KMP_EVENTS_PREFIX=rehydration \
    NATS_URL=nats://nats:4222

EXPOSE 50054 8080

USER kmp

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/kmp-server"]
