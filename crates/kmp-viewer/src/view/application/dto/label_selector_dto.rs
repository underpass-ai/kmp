//! A label predicate on the wire.

use serde::{Deserialize, Serialize};

/// One label predicate as the wire spells it — the same `{ key, op, values }`
/// every kernel read takes under `dimensions.selectors`, so an agent that
/// filters a read filters the loom with the same words. The operator is a
/// string here; the mapper refuses one outside the vocabulary.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelSelectorDto {
    /// The label key.
    pub key: String,
    /// `in`, `notin`, `exists` or `notexists`.
    pub op: String,
    /// The values `in` and `notin` compare; omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}
