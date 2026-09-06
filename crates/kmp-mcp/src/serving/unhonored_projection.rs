use std::collections::BTreeMap;

/// What a view intent asked for that this store or session cannot show.
#[derive(Debug, Default)]
pub(crate) struct UnhonoredProjection {
    pub(crate) dimensions: Vec<String>,
    pub(crate) overlays: Vec<String>,
    /// Label keys the about's catalogue does not hold.
    pub(crate) label_keys: Vec<String>,
    /// Per key, the `in`/`notin` values the catalogue does not hold under it.
    pub(crate) label_values: BTreeMap<String, Vec<String>>,
}

impl UnhonoredProjection {
    /// The label notes in the caller's words, one per key.
    pub(crate) fn label_notes(&self) -> Vec<String> {
        let mut notes = self
            .label_keys
            .iter()
            .map(|key| format!("label key `{key}` is not in this about's catalogue"))
            .collect::<Vec<_>>();
        notes.extend(self.label_values.iter().map(|(key, values)| {
            format!(
                "label `{key}` holds no value {} in this about",
                values
                    .iter()
                    .map(|value| format!("`{value}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }));
        notes
    }
}
