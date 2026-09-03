use serde_json::{Map, Value};

/// A memory as it is stored, read back before a write that attaches to it.
///
/// A summary is attached to an entry by writing the entry again with the
/// summary in its metadata. The text, the kind and every coordinate come
/// from the store and never from the caller, so the write cannot move the
/// memory an inch: the only thing the caller supplies is the rendering.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExistingEntry {
    pub(crate) reference: String,
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) coordinates: Vec<Value>,
    pub(crate) metadata: Map<String, Value>,
}

impl ExistingEntry {
    /// Reads the entry out of a `kmp_inspect` result that asked for details
    /// and raw coordinates.
    pub(crate) fn from_inspect(reference: &str, inspect: &Value) -> Result<Self, String> {
        let raw = inspect["raw"]
            .as_array()
            .and_then(|raw| {
                raw.iter()
                    .find(|item| item["ref"].as_str() == Some(reference))
            })
            .ok_or_else(|| {
                format!("`{reference}` was inspected but its raw record did not come back")
            })?;
        let text = inspect["object"]["text"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .or_else(|| raw["text"].as_str())
            .ok_or_else(|| format!("`{reference}` has no text to summarise"))?
            .to_string();
        let kind = raw["kind"]
            .as_str()
            .or_else(|| inspect["object"]["kind"].as_str())
            .filter(|kind| !kind.trim().is_empty())
            .unwrap_or("entry")
            .to_string();
        let coordinates = raw["coordinates"].as_array().cloned().unwrap_or_default();
        if coordinates.is_empty() {
            return Err(format!(
                "`{reference}` has no coordinates on record; a memory cannot be written back without one"
            ));
        }
        let metadata = inspect["object"]["metadata"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            reference: reference.to_string(),
            kind,
            text,
            coordinates,
            metadata,
        })
    }
}
