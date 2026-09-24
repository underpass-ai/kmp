//! What a wake packet says the memory currently holds.
//!
//! The state is the memories being resumed. The edges that tie a source to
//! a memory, or a memory to its lane, stay in the proof: when an about had
//! more `supports` edges than memories, ranking every non-structural
//! relation first filled the five state lines with bookkeeping and pushed
//! the decisions themselves out. A memory the lifecycle has replaced or
//! retired is still returned, after the live ones.

use std::collections::BTreeSet;

use kmp_application::RenderedContext;
use kmp_domain::{KmpBundle, RelationSemanticClass};

use super::memory_lifecycle::MemoryLifecycle;

const STATE_LINES: usize = 5;

/// Relations that record where a memory's support or placement comes from,
/// not what the memory says.
const SUPPORT_BOOKKEEPING: &[&str] = &["supports", "has_evidence", "records"];

pub(super) fn rendered_current_state(
    rendered: &RenderedContext,
    bundle: &KmpBundle,
    lifecycle: &MemoryLifecycle,
) -> Vec<String> {
    let entries = bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
        .map(|relationship| relationship.target_node_id())
        .collect::<BTreeSet<_>>();
    let mut explanatory = BTreeSet::new();
    let mut bookkeeping = BTreeSet::new();
    for relationship in bundle.relationships() {
        let id = format!(
            "rel:{}→{}",
            relationship.source_node_id(),
            relationship.target_node_id()
        );
        let structural =
            relationship.explanation().semantic_class() == &RelationSemanticClass::Structural;
        if structural || SUPPORT_BOOKKEEPING.contains(&relationship.relationship_type()) {
            bookkeeping.insert(id);
        } else {
            explanatory.insert(id);
        }
    }
    // Live memories first, straight from the graph: the rendered context is
    // budgeted and can drop the very nodes a resume needs.
    let (live, retired): (Vec<_>, Vec<_>) = bundle
        .neighbor_nodes()
        .iter()
        .filter(|node| entries.contains(node.node_id()) && !node.summary().trim().is_empty())
        .partition(|node| {
            !lifecycle.is_superseded(node.node_id()) && !lifecycle.is_expired(node.node_id())
        });
    // The title repeats the summary for most written memories; say it once.
    let memory_line = |node: &&kmp_domain::BundleNode| {
        let summary = node.summary().trim();
        let title = node.title().trim();
        if title.is_empty() || title == summary || summary.starts_with(title) {
            format!("({}) {summary}", node.node_kind())
        } else {
            format!("{title} ({}): {summary}", node.node_kind())
        }
    };
    let rank = |source_id: &str| {
        if explanatory.contains(source_id) {
            0
        } else if source_id.starts_with("detail:") {
            1
        } else if bookkeeping.contains(source_id) {
            3
        } else {
            2
        }
    };
    let entry_section = |source_id: &str| {
        source_id
            .strip_prefix("node:")
            .is_some_and(|node_id| entries.contains(node_id))
    };
    let mut sections = rendered
        .sections
        .iter()
        .filter(|section| !section.content.trim().is_empty())
        .filter(|section| !entry_section(&section.source_id))
        .collect::<Vec<_>>();
    // Stable: within a rank the renderer's order stands.
    sections.sort_by_key(|section| rank(&section.source_id));
    let state = live
        .iter()
        .map(memory_line)
        .chain(sections.into_iter().map(|section| section.content.clone()))
        .chain(retired.iter().map(memory_line))
        .take(STATE_LINES)
        .collect::<Vec<_>>();
    if state.is_empty() && !rendered.content.trim().is_empty() {
        vec![rendered.content.clone()]
    } else {
        state
    }
}
