//! Reading a memory back before a write attaches to it.
//!
//! The pre-read asks `kmp_inspect` for exactly what the write needs — the
//! stable object and the raw record — and for nothing that can crowd them
//! out. Inspect pages its expandable sections in a fixed order with the raw
//! record last, so a memory with enough links used to push its own raw
//! record onto a continuation page the pre-read never turned, and the write
//! refused a memory that was there (#497). Links are left out of the
//! request; and when the record still does not fit under the default
//! ceiling — evidence cannot be excluded, and a long text is repeated by its
//! raw record — the inspection is repeated once at the exact size the first
//! page reported, which the contract promises is enough.

use serde_json::{Value, json};

use crate::serving::ToolError;
use crate::serving::ports::kernel_tool_backend::KernelMcpToolBackend;
use crate::write::existing_entry::ExistingEntry;

/// The smallest ceiling `kmp_inspect` accepts.
const MINIMUM_INSPECT_BUDGET_BYTES: u64 = 512;

/// Reads `reference` out of the store as an attaching write needs it: the
/// stored text, kind, coordinates and metadata, from the object and its raw
/// record, with links left out of the inspection. `operation` names the
/// caller's shape so a failed pre-read says which write could not proceed.
pub(crate) async fn read_existing_entry(
    backend: &dyn KernelMcpToolBackend,
    operation: &str,
    about: &str,
    reference: &str,
) -> Result<ExistingEntry, ToolError> {
    let mut arguments = json!({
        "about": about,
        "ref": reference,
        "include": {"details": true, "raw": true, "incoming": false, "outgoing": false}
    });
    let mut inspected = inspect(backend, operation, reference, &arguments).await?;
    if !carries_raw_record(&inspected, reference)
        && let Some(required_bytes) = inspected["page"]["required_bytes"].as_u64()
    {
        arguments["budget"] = json!({
            "max_bytes": required_bytes.max(MINIMUM_INSPECT_BUDGET_BYTES)
        });
        inspected = inspect(backend, operation, reference, &arguments).await?;
    }
    ExistingEntry::from_inspect(reference, &inspected).map_err(ToolError::backend)
}

async fn inspect(
    backend: &dyn KernelMcpToolBackend,
    operation: &str,
    reference: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let result = backend
        .call_tool("kmp_inspect", arguments)
        .await
        .map_err(|mut error| {
            error.message = format!(
                "{operation} could not read `{reference}` before attaching to it: {}",
                error.message
            );
            error
        })?;
    Ok(result.get("structuredContent").cloned().unwrap_or(result))
}

fn carries_raw_record(inspected: &Value, reference: &str) -> bool {
    inspected["raw"].as_array().is_some_and(|raw| {
        raw.iter()
            .any(|item| item["ref"].as_str() == Some(reference))
    })
}
