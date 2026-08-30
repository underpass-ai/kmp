use serde_json::Value;

/// Everything one validated write intends: the canonical ingest arguments
/// it compiled to, and what the caller should read back afterwards.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KernelWritePlan {
    pub(crate) about: String,
    pub(crate) dry_run: bool,
    pub(crate) ingest_arguments: Value,
    pub(crate) generated_refs: Vec<String>,
    pub(crate) relations: Vec<String>,
    pub(crate) relation_quality: Vec<Value>,
    pub(crate) relation_quality_metrics: Value,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) next_suggested_reads: Vec<Value>,
}
