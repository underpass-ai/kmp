FROM rust:1.97.1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY api ./api
COPY crates ./crates
COPY vendor ./vendor

RUN cargo build --locked --release \
    -p rehydration-testkit --bin graph_batch_roundtrip \
    -p rehydration-testkit --bin graph_relation_roundtrip \
    -p rehydration-testkit --bin graph_batch_vllm_request

FROM docker.io/fullstorydev/grpcurl:v1.9.3 AS grpcurl

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash jq curl ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/e2e --shell /usr/sbin/nologin e2e

COPY --from=grpcurl /bin/grpcurl /usr/local/bin/grpcurl
COPY --from=builder /workspace/target/release/graph_batch_roundtrip /usr/local/bin/graph_batch_roundtrip
COPY --from=builder /workspace/target/release/graph_relation_roundtrip /usr/local/bin/graph_relation_roundtrip
COPY --from=builder /workspace/target/release/graph_batch_vllm_request /usr/local/bin/graph_batch_vllm_request

COPY api /app/api
COPY e2e/kernel-runner/entrypoint.sh /app/entrypoint.sh
COPY e2e/kernel-runner/common.sh /app/common.sh
COPY e2e/kernel-runner/tests/ /app/tests/

RUN chmod +x /app/entrypoint.sh /app/common.sh /app/tests/*.sh

USER e2e

ENTRYPOINT ["/usr/bin/tini", "--", "/app/entrypoint.sh"]
