//! An incremental continuation: the page that carries only new expansion
//! items because the caller already holds the first page's stable core.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::json_paths::array_at_mut;
use super::plan::{ProjectionItem, Section};

/// The marker a continuation sets in `projection` when it omits the core.
pub(super) const CORE_REUSED: &str = "core_reused";

/// Whether this page omits the core: a continuation past the first page
/// whose caller did not ask for the core again.
pub(super) fn reuses_core(offset: usize, repeat_core: bool) -> bool {
    offset > 0 && !repeat_core
}

/// The empty arrays a continuation may fill: every section with an eligible
/// item at or after `offset`. Sizing a page against all of them is a
/// conservative bound; `retain_expansion` drops those left empty.
pub(super) fn skeleton(eligible: &[&ProjectionItem], offset: usize) -> Value {
    let sections = eligible
        .iter()
        .skip(offset)
        .map(|item| item.section)
        .collect::<BTreeSet<_>>();
    let mut value = json!({});
    for section in sections {
        insert_at(&mut value, section.path(), json!([]));
    }
    value
}

/// Keep only what a continuation carries: the non-empty expansion arrays,
/// the progress block and the page's own warnings.
pub(super) fn retain_expansion(value: &mut Value) {
    let mut kept = json!({});
    for section in Section::ALL {
        if let Some(items) = array_at_mut(value, section.path())
            && !items.is_empty()
        {
            let items = std::mem::take(items);
            insert_at(&mut kept, section.path(), Value::Array(items));
        }
    }
    if let Some(object) = value.as_object_mut() {
        for key in ["projection", "warnings"] {
            if let Some(entry) = object.remove(key) {
                kept[key] = entry;
            }
        }
    }
    *value = kept;
}

fn insert_at(value: &mut Value, path: &[&str], leaf: Value) {
    let (last, parents) = path.split_last().expect("section path");
    let mut current = value;
    for key in parents {
        current = current
            .as_object_mut()
            .expect("section parent")
            .entry(*key)
            .or_insert_with(|| Value::Object(Map::new()));
    }
    current[*last] = leaf;
}
