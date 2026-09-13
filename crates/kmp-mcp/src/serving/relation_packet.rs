//! Reading both sources before a relation-only write links them.
//!
//! The compiler is pure, so the one thing it cannot do is prove that the
//! memories a link names are there. This reads every endpoint the about owns
//! before anything is compiled: a ref that is not in the store fails the
//! pre-read, and the packet is refused with no relation and no evidence
//! written. What comes back is also what is written straight back — same
//! text, same kind, same coordinates, same metadata — so the link arrives
//! without moving either source.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::serving::ToolError;
use crate::serving::existing_entry_read::read_existing_entry;
use crate::serving::ports::kernel_tool_backend::KernelMcpToolBackend;
use crate::write::plan::KernelWritePlan;
use crate::write::{build_relation_plan, declared_sources};

pub(super) async fn plan_relation_packet(
    backend: &dyn KernelMcpToolBackend,
    arguments: &Value,
) -> Result<KernelWritePlan, ToolError> {
    let (about, refs) = declared_sources(arguments)?;
    let mut sources = BTreeMap::new();
    for reference in refs {
        let existing = read_existing_entry(backend, "relations", &about, &reference).await?;
        sources.insert(reference, existing);
    }
    Ok(build_relation_plan(arguments, &sources)?)
}
