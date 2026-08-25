# Kernel Plugin Architecture

Status: first public Rust contract slice.

This document explains how kernel plugins are structured, where the stable
boundary lives, and how to implement a plugin without depending on the full
kernel.

## Design Goal

Plugins add domain interpretation above deterministic memory retrieval.

The kernel stores, indexes, traverses, traces, and inspects memory. Plugins
interpret retrieved evidence and can compute typed, auditable derivations such
as money totals, date differences, counts, latest-state resolution, or
domain-specific path policies.

This split keeps the memory substrate general:

- core remains independent from currency, date, count, preference, benchmark,
  and business semantics;
- plugins can evolve outside storage and traversal code;
- readers and agents get deterministic tools with explicit proof refs;
- humans can inspect which evidence was included or excluded.

## Crate Boundary

The plugin contract is owned by the kernel but exported as a lightweight crate:

| Crate or module | Responsibility | External plugin dependency? |
|:----------------|:---------------|:----------------------------|
| `kmp-plugin-api` | Public plugin API: evidence fragments, spans, typed values, derivation requests/results, plugin traits, plugin errors. | Yes. This is the preferred dependency for plugin crates. |
| `kmp-domain::plugins` | Kernel-domain re-export of `kmp-plugin-api` for internal cohesion. | Optional. Use only when already depending on domain. |
| `kmp-interpretation` | First reusable plugin implementations: money, date, value operations, text segmentation. | Optional. Use as reference or dependency when those implementations fit. |
| `kmp-testkit` | Benchmark probes and compatibility re-exports. | No. Testkit is not the plugin contract owner. |
| kernel application/adapters | Retrieve evidence, persist memories, expose gRPC/MCP, emit traces, write derived results. | No direct dependency from plugin implementations unless a plugin is deliberately application-owned. |

Dependency direction:

```text
external plugin crate
        |
        v
kmp-plugin-api

kmp-interpretation
        |
        v
kmp-plugin-api

kmp-domain::plugins
        |
        v
kmp-plugin-api
```

External plugins should not depend on:

- `kmp-application`;
- `kmp-domain`, unless they intentionally need broader domain types;
- storage adapters such as Neo4j, Valkey, NATS, or Postgres;
- gRPC/MCP transport crates;
- benchmark/testkit crates.

## Current Contract Shape

The first API slice exposes two plugin families.

### Value Plugins

Value plugins implement `EvidenceValuePlugin`.

They receive evidence fragments retrieved by the kernel and return typed value
mentions with byte spans and source refs.

```rust
use kmp_plugin_api::{
    EvidenceInterpretationInput, EvidenceInterpretationOutput, EvidenceValuePlugin,
    InterpretationError,
};

pub struct MyValuePlugin;

impl EvidenceValuePlugin for MyValuePlugin {
    fn id(&self) -> &'static str {
        "my-value-plugin-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        // Inspect input.fragments and return typed mentions.
        todo!()
    }
}
```

Value plugins should extract facts, not answer questions. A money amount, date,
or entity mention is only a typed mention until an upstream reader or agent
marks it as an operand for a specific derivation.

The trait is `Send + Sync` because kernel hosts may call plugins from
multi-threaded async services. If a plugin owns heavy or non-thread-safe state,
wrap that state behind a thread-safe adapter in the plugin crate.

### Derivation Plugins

Derivation plugins implement `EvidenceDerivationPlugin`.

They receive a `DerivationRequest` with an explicit operation and explicit
operands. Each operand is labeled as `include`, `exclude`, or `context`.

```rust
use kmp_plugin_api::{
    DerivationRequest, DerivationResult, EvidenceDerivationPlugin, InterpretationError,
};

pub struct MyDerivationPlugin;

impl EvidenceDerivationPlugin for MyDerivationPlugin {
    fn id(&self) -> &'static str {
        "my-derivation-plugin-v1"
    }

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError> {
        // Validate operation and operands, then return answer/value/proof refs.
        todo!()
    }
}
```

Derivation plugins own deterministic computation and validation. They should
fail fast when the request is invalid instead of guessing.

Examples:

- a currency plugin must reject mixed currencies unless it explicitly supports
  conversion and records the conversion source;
- a date plugin must reject date sums;
- a count plugin must define dedupe keys separately from numeric quantities;
- a latest/current plugin must explain superseded evidence.

The trait is also `Send + Sync` for the same host-runtime reason.

## Runtime Flow

The kernel and plugin responsibilities meet through evidence refs.

1. A client calls the kernel API, for example `Ask`, `Goto`, `Trace`, or
   another retrieval path.
2. The kernel returns deterministic context/evidence with stable refs.
3. The reader or agent builds `EvidenceInterpretationInput` from those refs.
4. One or more value plugins extract typed mentions.
5. The reader or agent selects operands for the user question.
6. A derivation plugin computes the result.
7. The caller may return the derivation directly or write a derived memory back
   with `derived_from` provenance.

The important rule is that operand selection is explicit. The plugin should not
silently decide that every matching value is relevant to the current question.

## Minimal External Plugin Crate

Recommended `Cargo.toml` shape:

```toml
[package]
name = "my-kernel-plugin"
version = "0.1.0"
edition = "2024"
rust-version = "1.90"

[dependencies]
kmp-plugin-api = { git = "https://github.com/underpass-ai/kmp" }
serde = { version = "1", features = ["derive"] }
```

When developing inside the workspace, use the path dependency:

```toml
kmp-plugin-api = { path = "../kmp-plugin-api" }
```

A plugin implementation should expose concrete types and keep construction
simple:

```rust
pub use kmp_plugin_api::{
    EvidenceDerivationPlugin, EvidenceValuePlugin, InterpretationError,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct MyDomainPlugin;
```

If the plugin needs heavy resources, such as a model, dictionary, or compiled
normalizer, keep those resources behind the plugin constructor. Do not make the
kernel own that infrastructure.

## Complete Minimal Derivation Plugin

This example implements a reusable unique-entity counter. It does not depend on
the kernel domain, storage, transport, or testkit crates.

```rust
use std::collections::BTreeSet;

use kmp_plugin_api::{
    DerivationOperation, DerivationRequest, DerivationResult, EvidenceDerivationPlugin,
    InterpretationError, InterpretedValue, OperandLabel,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct UniqueEntityCountPlugin;

impl EvidenceDerivationPlugin for UniqueEntityCountPlugin {
    fn id(&self) -> &'static str {
        "unique-entity-count-v1"
    }

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError> {
        if request.operation != DerivationOperation::Count {
            return Err(InterpretationError::new(format!(
                "{} only supports count derivations",
                self.id()
            )));
        }

        let mut included = BTreeSet::new();
        let mut included_refs = Vec::new();
        let mut excluded_refs = Vec::new();

        for operand in &request.operands {
            match operand.label {
                OperandLabel::Include => {
                    let key = operand
                        .entity
                        .as_deref()
                        .or(operand.raw.as_deref())
                        .unwrap_or(operand.ref_id.as_str());
                    if included.insert(key.to_string()) {
                        included_refs.push(operand.ref_id.clone());
                    }
                }
                OperandLabel::Exclude => excluded_refs.push(operand.ref_id.clone()),
                OperandLabel::Context => {}
            }
        }

        if included_refs.is_empty() {
            return Err(InterpretationError::new(
                "unique entity count requires included operands",
            ));
        }

        let count = included_refs.len();
        Ok(DerivationResult {
            plugin: self.id().to_string(),
            operation: request.operation,
            answer: Some(count.to_string()),
            value: Some(InterpretedValue::number(
                count as f64,
                Some("items".to_string()),
            )),
            included_refs,
            excluded_refs,
            diagnostics: Vec::new(),
        })
    }
}
```

Host-side invocation stays explicit:

```rust
use kmp_plugin_api::{
    DerivationOperand, DerivationOperation, DerivationRequest, EvidenceDerivationPlugin,
    InterpretedValue, OperandRole,
};

let plugin: Box<dyn EvidenceDerivationPlugin> = Box::new(UniqueEntityCountPlugin);

let request = DerivationRequest {
    question: "How many unique services were affected?".to_string(),
    operation: DerivationOperation::Count,
    unit: Some("services".to_string()),
    operands: vec![
        DerivationOperand::included(
            "turn:1",
            InterpretedValue::number(1.0, Some("service".to_string())),
        )
        .with_role(OperandRole::CountedItem)
        .with_entity("payments-api"),
        DerivationOperand::included(
            "turn:2",
            InterpretedValue::number(1.0, Some("service".to_string())),
        )
        .with_role(OperandRole::CountedItem)
        .with_entity("payments-worker"),
    ],
};

let result = plugin.derive(&request)?;
```

The host decides when to call the plugin and which evidence becomes operands.
The plugin only validates and computes.

## Host Wiring

There is no runtime registry in this slice. A host wires plugins at compile
time or application startup.

```rust
use kmp_plugin_api::{EvidenceDerivationPlugin, EvidenceValuePlugin};

pub struct PluginSet {
    pub value_plugins: Vec<Box<dyn EvidenceValuePlugin>>,
    pub derivation_plugins: Vec<Box<dyn EvidenceDerivationPlugin>>,
}
```

For the common in-process path, `kmp-interpretation` provides
`ComposedEvidenceReader` plus `EvidenceReaderPluginConfigurator`. This is the
kernel-owned reference reader for this slice: the configurator owns which
plugins are enabled and in which order, then the reader applies value plugins,
aggregates typed mentions and diagnostics, and routes explicit derivation
requests to the named derivation plugin.

```rust
use kmp_interpretation::{
    ComposedEvidenceReader, EvidenceFragment, EvidenceInterpretationInput, EvidenceReaderRequest,
    UrlValuePlugin, MoneyValuePlugin,
};

let reader = ComposedEvidenceReader::configure()
    .with_value_plugin(UrlValuePlugin)
    .with_value_plugin(MoneyValuePlugin)
    .build()?;

let evidence = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
    "turn:42",
    "Paid $120 on 2026-05-01. See https://example.test/runbook.",
)]);

let output = reader.read(&EvidenceReaderRequest::new(evidence))?;
assert_eq!(
    output.execution_order.iter().map(|step| step.plugin_id).collect::<Vec<_>>(),
    vec!["url-value-v1", "money-value-v1"]
);
```

Plugin order is part of the reader contract. It is serialized in
`plugin_configuration.plugin_order` and in per-read `execution_order`. Value
plugin order controls execution and output ordering. Derivation plugin
registration order controls the configured allow-list; derivation execution
order follows the explicit derivation steps in the reader request.

The composed reader intentionally does not infer that every extracted value is
an operand. Derivations must be requested explicitly with a plugin id and an
already labeled `DerivationRequest`. This keeps operand selection auditable and
prevents benchmark-specific heuristics from becoming kernel behavior.

If a later application needs discovery, sandboxing, version negotiation, or
remote execution, that belongs in a host/adapter layer above this contract.

## Implementation Checklist

For a value plugin:

- define a stable plugin id ending in a version, for example
  `currency-value-v1`;
- preserve original `ref_id`;
- preserve original text in `raw`;
- return byte offsets in `TextSpan`;
- normalize into `InterpretedValue` or a future typed extension;
- emit diagnostics for ignored ambiguous cases;
- avoid question-dependent inclusion.

For a derivation plugin:

- accept only supported `DerivationOperation` values;
- validate operand types before computing;
- preserve `included_refs` and `excluded_refs`;
- return structured `value` when possible, not only string `answer`;
- return diagnostics for abstention or weak evidence;
- fail fast on invalid requests.

For both:

- no hidden network calls in deterministic derivations;
- no dependency on storage adapters;
- no raw prompt or secret logging;
- tests for happy path, invalid operands, mixed-domain operands, and empty
  evidence.

## Writer-Side Use

Writer-side plugins can enrich incoming memories before or during ingestion.

Good writer-side responsibilities:

- detect typed mentions such as currency, dates, durations, quantities, code,
  URLs, and entities;
- attach typed metadata or derived nodes with provenance;
- build search/index helper fields;
- reject malformed explicit domain values early.

Writer-side plugins must not create query-specific conclusions. For example,
finding `USD 120` in a memory does not mean it should be summed. It only means
there is a money mention at a known ref/span.

## Reader-Side Use

Reader-side plugins run after retrieval.

Good reader-side responsibilities:

- extract typed mentions from retrieved evidence;
- let an LLM, deterministic selector, or UI mark operands;
- compute deterministic operations;
- return proof fields that explain included and excluded refs;
- optionally write derived results back as new memory with provenance.

Reader-side plugins are the right place for operations such as:

- "total spent";
- "days between two events";
- "current preference after updates";
- "count unique affected services";
- "best path under cost/risk/time criteria".

## Not In This Slice

The current plugin architecture is not yet:

- a dynamic plugin loader;
- a stable C ABI;
- a WASM host;
- a network plugin protocol;
- a runtime plugin marketplace;
- a registry with discovery, capability negotiation, and sandboxing.

Those can be added later without changing the main principle: plugin crates
depend on a small kernel-owned API, and the kernel core stays independent from
domain operators.

## Existing Implementations

`kmp-interpretation` currently provides the reference implementations:

- `ComposedEvidenceReader`;
- `MoneyValuePlugin`;
- `DateValuePlugin`;
- `MathExpressionValuePlugin`;
- `SourceCodeValuePlugin`;
- `UrlValuePlugin`;
- `CurrencyDerivationPlugin`;
- `DateDerivationPlugin`;
- `ValueOperationPlugin`;
- `TextNormalizationPipeline`.

These are intentionally outside kernel core. They demonstrate how to implement
the public API while keeping arithmetic, date math, and text segmentation above
the memory substrate.

`MathExpressionValuePlugin` detects math spans and emits
`InterpretedValue::MathExpression` with delimiter notation and the clean
expression body. It is intentionally not an evaluator: arithmetic, symbolic
solving, or operand selection must be explicit derivations above the extracted
value.

`SourceCodeValuePlugin` is deliberately simple: it detects code spans, preserves
the original `TextSpan`, and emits `InterpretedValue::SourceCode` with
`language`, `segment_kind`, and the clean code `text`. The `raw` mention still
keeps the original fenced or inline segment for audit. The reader can then
treat that evidence as code without the kernel core learning
programming-language semantics.

The segment-precedence model lives in `EvidenceSegmentKind`: `source_code`,
`math`, `url`, then `text`. Plugin application order must not change what a
reader sees. Source-code plugins consume source-code spans, math plugins consume
math spans, URL plugins consume URL spans, and money/date plugins consume text
spans only.

`UrlValuePlugin` follows the same pattern for links: it preserves the original
span/ref and emits `InterpretedValue::Url { url }`, so readers can treat links
as links instead of rediscovering them from prose.
