FROM rust:1.97.1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY api ./api
COPY crates ./crates

RUN cargo build --locked --release \
    -p rehydration-server --bin rehydration-server \
    -p rehydration-transport-grpc --bin runtime_reference_client

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/rehydration --shell /usr/sbin/nologin rehydration

COPY --from=builder /workspace/target/release/rehydration-server /usr/local/bin/rehydration-server
COPY --from=builder /workspace/target/release/runtime_reference_client /usr/local/bin/runtime-reference-client

ENV REHYDRATION_SERVICE_NAME=rehydration-kernel \
    REHYDRATION_GRPC_BIND=0.0.0.0:50054 \
    REHYDRATION_GRAPH_URI=neo4j://neo4j:7687 \
    REHYDRATION_DETAIL_URI=redis://valkey:6379 \
    REHYDRATION_SNAPSHOT_URI=redis://valkey:6379 \
    REHYDRATION_RUNTIME_STATE_URI=redis://valkey:6379 \
    REHYDRATION_EVENTS_PREFIX=rehydration \
    NATS_URL=nats://nats:4222

EXPOSE 50054

USER rehydration

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/rehydration-server"]
