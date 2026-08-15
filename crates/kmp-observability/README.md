# kmp-observability

Telemetry wiring for [KMP by Underpass](https://github.com/underpass-ai/kmp),
the Kernel Memory Protocol kernel.

Two things live here: the OpenTelemetry pipeline (tracing subscriber, OTLP
export over gRPC, optional mutual TLS to the collector, meter provider) and
the quality observers the kernel reports through — relation quality, scope
behaviour, write and projection outcomes.

## Features

`otel` is on by default and carries the OpenTelemetry stack. Turning it off
(`default-features = false`) drops that whole dependency tree and leaves the
buffered in-memory observer, which is what the embedded edition uses when
nobody is collecting anything. The kernel behaves the same either way;
telemetry is never load-bearing.

## Stability

Published so the rest of the kernel can be published, not as a curated public
API. It moves with the kernel's releases.

## License

Apache-2.0.
