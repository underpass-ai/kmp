//! Which loom a caller means.

/// The view a host opens when it does not name one — one window, one loom.
pub const DEFAULT_VIEW_ID: &str = "default";

/// The identity of one open view. Ids are opaque: the registry keys on them
/// and nothing ever parses one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(String);

impl ViewId {
    /// The id a caller named, or [`DEFAULT_VIEW_ID`] when they named none.
    pub fn or_default(name: Option<&str>) -> Self {
        Self(name.unwrap_or(DEFAULT_VIEW_ID).to_string())
    }

    /// The id as the caller spelled it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ViewId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<&str> for ViewId {
    fn from(name: &str) -> Self {
        Self(name.to_string())
    }
}
