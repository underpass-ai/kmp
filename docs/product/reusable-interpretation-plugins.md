# Reusable Interpretation Plugins

Status: first reusable plugin slice.

For the implementation architecture and external plugin authoring guide, see
[`kernel-plugin-architecture.md`](kernel-plugin-architecture.md).

The kernel retrieves deterministic evidence. Interpretation plugins consume that
evidence and produce typed, auditable derivations. This keeps benchmark,
business, and domain operators outside kernel core while still making them easy
to reuse from readers, agents, and evaluation harnesses.

Current integration status, 2026-05-09:

- `kmp-plugin-api` and `kmp-interpretation` are implemented.
- `ComposedEvidenceReader` is implemented for in-process readers and benchmark
  adapters.
- KMP/gRPC/MCP `kmp_ask` does not automatically run interpretation plugins.
  It remains deterministic evidence retrieval. A reader, agent, benchmark
  harness, SDK, or future helper must explicitly compose plugins over returned
  evidence.
- Operand selection is still caller-owned. Plugins can compute from typed
  operands, but they do not decide by themselves which retrieved values answer a
  question.

## Boundary

Kernel responsibilities:

- ingest memories;
- preserve `about`, dimension, temporal, relation, trace, and inspect state;
- retrieve evidence with stable refs.

Plugin responsibilities:

- interpret evidence values;
- decide which extracted values are operands only when an upstream reader or
  agent marks them as `include`, `exclude`, or `context`;
- normalize currency, dates, units, and entities;
- run deterministic operations over typed operands;
- return answer, value, refs, excluded refs, and diagnostics.

Currency and dates are treated as small domains, not loose scalar types. Plugins
must fail fast on incompatible operands. For example, a currency sum cannot
silently mix `USD` and `EUR`; a date sum is invalid; a date difference returns a
typed number in days with source refs.

## Reader And Writer Use

The domain plugins are shared code. They are not owned by the reader or the
writer.

Writer-side use:

- parse incoming memories for typed mentions such as currency amounts and dates;
- attach typed metadata or derived value nodes with provenance;
- build search/index fields for later retrieval;
- reject malformed explicit domain values early.

Reader-side use:

- consume kernel evidence refs;
- let a reader or agent decide which mentions are operands for the current
  question;
- compute query-time derivations such as monetary totals or elapsed days;
- return a proof object with included and excluded refs.

The writer must not invent question-dependent operations. A dollar amount found
in a memory is only a typed currency mention. It becomes a sum operand only when
a reader, agent, or explicit upstream event marks it as such.

## Kernel Plugin Boundary

The boundary is split into a lightweight API crate, a domain re-export, and
implementation crates:

- `crates/kmp-plugin-api`: kernel-owned public crate for plugin
  authors. It defines evidence fragments, spans, interpreted values, derivation
  requests/results, plugin traits, and plugin errors. External plugins should
  depend on this crate, not on the whole kernel.
- `crates/kmp-domain/src/plugins`: internal domain-facing re-export as
  `kmp_domain::plugins`. Use this path only when a kernel crate already
  depends on `kmp-domain`.
- `crates/kmp-interpretation`: plugin implementations that import and
  implement `kmp-plugin-api`.
- `crates/kmp-testkit`: benchmark and fixture consumers. It may
  re-export the interpretation APIs temporarily for compatibility, but it is
  not the owner of the plugin contract or implementations.

Text normalization lives in
`crates/kmp-interpretation/src/text_normalization.rs`. The first cut is only a
deterministic span segmenter:

- normal text spans;
- protected code spans, including fenced and inline Markdown code;
- protected math spans, including LaTeX-style inline/block delimiters;
- protected URL spans;
- normalized text for comparison while preserving original byte offsets.

The segment classes are modeled in the plugin API as `EvidenceSegmentKind`.
Precedence is deterministic and independent from plugin execution order:
`source_code -> math -> url -> text`. Value plugins consume the segment classes
they own. For example, source-code plugins read source-code spans, math plugins
read math spans, URL plugins read URL spans, and money/date plugins read text
spans only.

Money, date, and scalar-value extraction remain separate plugins. They consume
the shared segmentation instead of each plugin inventing its own protected-text
rules.

Implemented domain plugins:

- `CurrencyDerivationPlugin`: owns currency extraction plus monetary
  operations. It normalizes values into `InterpretedValue::Money` with ISO-like
  currency code, decimal amount, and minor units.
- `DateDerivationPlugin`: owns date extraction plus temporal operations. It
  normalizes values into `InterpretedValue::Date` and supports temporal
  difference, latest/max-by, list, and explicit abstention.
- `MathExpressionValuePlugin`: detects LaTeX-style math segments and returns
  `InterpretedValue::MathExpression` with the original span, delimiter notation,
  and clean expression body. It does not evaluate the expression or decide
  operands.
- `SourceCodeValuePlugin`: detects Markdown code segments and returns
  `InterpretedValue::SourceCode` with the original span, segment kind, and
  language when declared or conservatively inferred. The value also carries the
  clean code text while the mention `raw` keeps the original fenced or inline
  segment. This lets a reader know that the evidence it is reading is source
  code instead of ordinary prose.
- `UrlValuePlugin`: detects URL segments and returns `InterpretedValue::Url`
  with the URL text. The mention keeps the original ref/span so a reader can
  distinguish links from ordinary prose without parsing raw text itself.

Lower-level primitives:

- `MoneyValuePlugin`: extracts money mentions such as `$1,200.50`, `EUR 30`,
  `30 dollars`, and normalizes them into `InterpretedValue::Money`.
- `DateValuePlugin`: extracts ISO-like dates and simple named dates such as
  `2026-05-06`, `05/06/2026`, and `May 6, 2026`.
- `MathExpressionValuePlugin`: extracts protected math expressions from
  normalized math spans.
- `UrlValuePlugin`: extracts URL mentions from normalized URL spans.
- `ValueOperationPlugin`: shared deterministic primitive used by domain
  plugins. It is not the product-facing plugin boundary.

The operation input is deliberately separate from extraction. A money mention is
not automatically a sum operand. The reader or agent must create a derivation
request that labels each candidate as `include`, `exclude`, or `context`.

## Composed Reader

`kmp-interpretation` exposes `ComposedEvidenceReader` as the generic
reader base for in-process kernel consumers. It composes plugin lists instead
of hard-coding a benchmark or domain flow:

- `EvidenceReaderPluginConfigurator` is the only intended construction path
  for custom reader composition;
- value plugins are executed in configured order over the same evidence input;
- typed mentions, per-plugin outputs, and diagnostics are aggregated;
- derivation plugins are invoked only when the request names a registered
  `plugin_id`;
- duplicate plugin ids fail fast during reader construction;
- configured order is emitted through `plugin_configuration.plugin_order`;
- actual per-read execution order is emitted through `execution_order`.

Order matters. A host must choose it deliberately instead of relying on
incidental `Vec` construction. The default order is stable and auditable, but a
host can construct another order when a domain needs a different read policy.
Span-class precedence remains deterministic and independent from reader order:
source-code spans, math spans, URL spans, and text spans are segmented before
value plugins interpret their owned spans.

The default kernel reader currently wires all reusable base value plugins:

- `SourceCodeValuePlugin`;
- `MathExpressionValuePlugin`;
- `UrlValuePlugin`;
- `MoneyValuePlugin`;
- `DateValuePlugin`.

It also registers deterministic derivation plugins:

- `ValueOperationPlugin`;
- `CurrencyDerivationPlugin`;
- `DateDerivationPlugin`.

This is the path benchmark adapters should consume. A benchmark may still build
domain-specific operand-selection logic above the reader, but that logic must
remain outside the kernel plugin base.

## Minimal Flow

1. Ask the kernel for context and keep the refs.
2. Run value extractors on the retrieved evidence.
3. Let a reader or agent select operands against the question.
4. Run the matching domain plugin, for example `CurrencyDerivationPlugin` or
   `DateDerivationPlugin`.
5. Optionally write the derived result back to memory with provenance
   `derived_from: [refs...]`.

Example shape:

```rust
use kmp_interpretation::{
    CurrencyDerivationPlugin, DerivationOperand, DerivationOperation,
    DerivationRequest, EvidenceFragment, EvidenceInterpretationInput,
};

let evidence = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
    "turn:42",
    "Paid $120 on 2026-05-01 and $65 on 2026-05-06.",
)]);

let currency = CurrencyDerivationPlugin;
let money = currency.interpret(&evidence)?;

let request = DerivationRequest {
    question: "How much was spent?".to_string(),
    operation: DerivationOperation::Sum,
    unit: None,
    operands: money
        .values
        .into_iter()
        .map(|mention| DerivationOperand::included(mention.ref_id, mention.value))
        .collect(),
};

let result = currency.derive(&request)?;
assert_eq!(result.answer.as_deref(), Some("USD 185"));
```

## Next Plugin Families

P0 next:

- counting plugin: counted entities, dedupe keys, quantity-vs-cardinality
  separation;
- unit and duration plugin: hours, days, weeks, quantities, rates;
- entity canonicalization plugin: same vendor/person/item across sessions;
- latest/current plugin: resolve superseded facts and active preferences;
- preference/state plugin: current likes, constraints, disallowed options;
- candidate-lifecycle helper: candidate, positive operand, negative operand,
  superseded candidate, context-only.

These remain above the kernel. The kernel should never learn what currency
arithmetic, date math, counting, or preference deduplication means.
