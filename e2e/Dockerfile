# Legacy gRPC-only E2E test runner for kmp.
# Select test via TEST_ID env var (01, 02, 03, 04).
# All tests use grpcurl with mTLS — fail-fast, no fallbacks.
# For async GraphBatch + NATS + gRPC contract coverage, use e2e/kernel-runner/.
FROM docker.io/fullstorydev/grpcurl:v1.9.3 AS grpcurl

FROM docker.io/library/alpine:3.21
RUN apk add --no-cache ca-certificates bash jq
COPY --from=grpcurl /bin/grpcurl /usr/local/bin/grpcurl

COPY e2e/tests/ /app/tests/
RUN chmod +x /app/tests/*.sh

USER 65534
CMD ["sh", "-c", "exec /app/tests/test-${TEST_ID}.sh"]
