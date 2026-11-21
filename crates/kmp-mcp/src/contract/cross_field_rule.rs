//! The cross-field rules of the call contract: what one argument requires
//! of another. They are enforced on every call from the full contract, where
//! each one is a JSON-Schema conditional tagged with its id (`x-kmp-rule`),
//! but `tools/list` does not publish them: a conditional cost every session
//! tokens and some hosts render a schema with one as untyped. They are
//! stated here once, served by `kmp_guide` in the card of the topic that
//! owns the argument, and named with their fix in the refusal.

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::contract::schema::relation_vocabulary::writer_relations_requiring_class;

/// The schema keyword that tags a conditional with the rule it enforces.
pub(crate) const RULE_KEYWORD: &str = "x-kmp-rule";

/// One documented rule as one tool applies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CrossFieldRule {
    pub(crate) id: &'static str,
    pub(crate) tool: &'static str,
    /// The argument the rule is about; it decides which guide topic serves
    /// the rule. `""` is the call as a whole.
    pub(crate) path: &'static str,
    pub(crate) rule: String,
    pub(crate) fix: String,
}

/// The tools that resume a returned call from a `continuation` handle.
pub(crate) const CONTINUATION_TOOLS: [&str; 7] = [
    "kmp_wake",
    "kmp_ask",
    "kmp_inspect",
    "kmp_trace",
    "kmp_relate",
    "kmp_time",
    "kmp_write_memory",
];

/// Tags `schema` with the rule `id`: a failure inside it is reported as that
/// rule, with its fix. `schema` must hold the conditional and nothing else.
pub(crate) fn tagged(id: &str, mut schema: Value) -> Value {
    schema[RULE_KEYWORD] = json!(id);
    schema
}

fn rule(
    id: &'static str,
    tool: &'static str,
    path: &'static str,
    rule: &str,
    fix: &str,
) -> CrossFieldRule {
    CrossFieldRule {
        id,
        tool,
        path,
        rule: rule.to_string(),
        fix: fix.to_string(),
    }
}

/// Every documented rule, once per tool that applies it.
pub(crate) fn all() -> &'static [CrossFieldRule] {
    static RULES: OnceLock<Vec<CrossFieldRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let class = writer_relations_requiring_class()
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let (destinations, evidence) = crate::contract::tools::trace::search_mode_fields();
        let mut rules = Vec::new();
        for tool in CONTINUATION_TOOLS {
            rules.push(rule(
                "continuation-alone",
                tool,
                "continuation",
                "A `continuation` is sent alone, exactly as returned; beside it only the `about` or `actor` it already binds (Wake and Ask also take `page`).",
                "Send `{\"continuation\":\"…\"}` alone; to change an argument, send the original call again.",
            ));
        }
        rules.extend([
            rule(
                "write.one-form",
                "kmp_write_memory",
                "",
                "A write carries exactly one of `memories`, `relations` or `search_summaries`.",
                "Send one call per form.",
            ),
            rule(
                "write.labels",
                "kmp_write_memory",
                "labels",
                "With `memories`, every memory needs a label: `labels` on the packet or on that memory.",
                "Add `labels`, e.g. {\"topic\":[\"x\"]}, to the packet or to the memory.",
            ),
            rule(
                "write.evidence",
                "kmp_write_memory",
                "memories[].evidence",
                "Every memory needs `evidence` unless `options.strict` is false.",
                "Add the source text or observation behind it as `evidence`.",
            ),
            rule(
                "write.link-class",
                "kmp_write_memory",
                "memories[].connect_to",
                &format!("A link with rel {class} needs `class`, in `connect_to` and in `relations`."),
                "Add the `class` the source justifies.",
            ),
            rule(
                "write.relations-form",
                "kmp_write_memory",
                "relations",
                "A `relations` write takes no packet `labels`, `occurred_at`, `valid_from`, `valid_until`, `rank`, `options.sequence` or `options.labels_new`.",
                "Remove them: record a new fact with `memories`, change labels with kmp_relabel.",
            ),
            rule(
                "write.summaries-form",
                "kmp_write_memory",
                "search_summaries",
                "A `search_summaries` write takes no `labels`, `occurred_at`, `valid_from`, `valid_until`, `rank`, `read_context`, `review_token`, `options.sequence` or `options.labels_new`.",
                "Remove them: it only attaches renderings to memories already stored.",
            ),
            rule(
                "write.summary-content",
                "kmp_write_memory",
                "search_summaries",
                "Each `search_summaries` item carries `summary_en` or `search_expansions`.",
                "Add the English rendering or the expansions to the item.",
            ),
            rule(
                "time.cursor",
                "kmp_time",
                "move",
                "`rewind` and `forward` take `from` or `interval`; `goto` takes `at`; `near` takes `around`.",
                "Add the cursor this move takes.",
            ),
            rule(
                "trace.mode",
                "kmp_trace",
                "",
                "A trace gives `to` (known destinations) or `search.seek` (evidence from the seed): one of them, never both.",
                "Keep `to` and drop `search.seek`, or keep `search.seek` and drop `to`.",
            ),
            rule(
                "trace.search-mode",
                "kmp_trace",
                "search",
                &format!(
                    "Inside `search`, destination options ({}) never meet evidence options ({}); evidence mode needs `seek`.",
                    destinations.join(", "),
                    evidence.join(", ")
                ),
                "Keep the options of one mode.",
            ),
            rule(
                "ingest.observed",
                "kmp_ingest",
                "provenance.observed_at",
                "`provenance.observed_at` is required unless `default_observation_to_ingestion` is true.",
                "Give the RFC3339 observation, or set `default_observation_to_ingestion: true`.",
            ),
            rule(
                "ingest.links-only",
                "kmp_ingest",
                "memory.entries",
                "A memory with no `entries` attaches at least one relation.",
                "Add `memory.relations`, or the entries.",
            ),
            rule(
                "condense.expect",
                "kmp_condense",
                "expect",
                "`expect` is `{\"absent\":true}` for a first card or `{\"card_revision\":n}` to replace one, never both.",
                "Keep the one that matches the stored card.",
            ),
        ]);
        rules
    })
}

/// The rule `id` as `tool` applies it.
pub(crate) fn named(tool: &str, id: &str) -> Option<&'static CrossFieldRule> {
    all().iter().find(|rule| rule.tool == tool && rule.id == id)
}

/// The rule wording a refusal carries: what the rule says, what this call
/// did against it, and the fix.
pub(crate) fn refusal_wording(tool: &str, id: &str, detail: &str) -> String {
    match named(tool, id) {
        Some(rule) => format!("Rule `{id}`: {} {detail} Fix: {}", rule.rule, rule.fix),
        None => format!("Rule `{id}`: {detail}"),
    }
}
