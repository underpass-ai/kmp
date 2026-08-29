use std::collections::BTreeSet;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
pub(crate) enum StringOrListDto {
    #[default]
    Missing,
    String(String),
    List(Vec<String>),
}

impl StringOrListDto {
    pub fn into_set(self, split_spaces: bool) -> BTreeSet<String> {
        match self {
            Self::Missing => BTreeSet::new(),
            Self::String(value) if split_spaces => value
                .split_ascii_whitespace()
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
            Self::String(value) => value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
            Self::List(values) => values
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        }
    }
}
