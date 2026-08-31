//! Which memory the loom is woven over.

/// The anchor a view is opened onto. Abouts are opaque routing identifiers:
/// this context stores and compares them and never parses one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutId(String);

impl AboutId {
    /// An about exactly as the caller spelled it.
    pub fn new(about: impl Into<String>) -> Self {
        Self(about.into())
    }

    /// The about, byte for byte.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
