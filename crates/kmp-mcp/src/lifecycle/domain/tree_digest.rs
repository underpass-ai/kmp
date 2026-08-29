use std::fmt;

use serde::Serialize;

/// Content-and-path digest of a complete installed plugin tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TreeDigest(String);

impl TreeDigest {
    pub fn sha256(hex: String) -> Self {
        Self(format!("sha256:{hex}"))
    }
}

impl fmt::Display for TreeDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
