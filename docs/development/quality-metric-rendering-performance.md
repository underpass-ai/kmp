# Quality metric rendering performance (#768)

`BundleQualityMetrics::compute` reports exact metrics for the selected bundle. Its
`raw_equivalent_tokens` value is the cl100k token count of the canonical flat
records, while `compression_ratio` compares that count with the structured
rendered token count. The value must continue to describe the same concatenated
UTF-8 text, including Unicode, escaping, newlines, and optional relationship
fields.

The production path admits and materializes the selected recall bundle before
calling `render_graph_bundle_with_options`. The application render constructs
the bounded structured response and computes quality from that same selected
bundle. Embedded ask/wake and gRPC adapters observe the resulting quality
telemetry. The typed recall projection passes an estimator argument to its
byte-bounded projection helper, but that helper does not tokenize the response.

The implementation emits the flat representation as records. The default
`TokenEstimator` method joins records exactly, preserving behavior for custom
estimators. `Cl100kEstimator` keeps a pending record buffer and flushes only when
the previous record ends in LF and the next record begins with an ASCII letter.
The cl100k regex separates trailing whitespace from the next ASCII word at this
boundary. Unsafe boundaries remain pending and are concatenated, so arbitrary
callers retain full-string semantics. The normal canonical records therefore
avoid one full raw-dump `String`; the pending buffer can coexist with the next
record and the token vector for the current encode operation. Unusual unsafe
inputs may accumulate more records by design.

Tests compare record output with an independent legacy flat-dump oracle and
compare cl100k record counts with full-string counts over Unicode, escaping,
newlines, empty records, optional relationship fields, and unsafe boundaries.

The performance runner separates structured rendering from metric-only work and
keeps first-use and warm samples distinct. It also repeats each phase under the
existing Linux/glibc allocation control; instrumented timings are discarded.
The four synthetic shapes cover small, Unicode/escaping, approximately 600 KB
raw payload, and relation-heavy bundles. The runner asserts complete rendered
context equality across warm calls and emits a debug fingerprint, while quality
fields are compared exactly. The report records baseline and candidate binary
hashes, source hashes, toolchain, host, sample counts, nearest-rank percentiles,
and raw sample arrays.

This change removes the full raw-dump buffer from the normal canonical metric
path. It still performs the complete exact BPE tokenization required by the
metric contract; changing that contract to a heuristic or moving telemetry
after response projection would measure a different quantity.
