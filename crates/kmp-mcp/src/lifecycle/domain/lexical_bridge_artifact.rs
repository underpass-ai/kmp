use super::lifecycle_error::LifecycleError;

/// Checksum-verified bytes of one lexical-bridge table.
///
/// The table teaches `kmp_ask` that `válvula` and `valve` are the same word,
/// so a question asked in one language reaches a memory written in another.
/// It is an aid to retrieval and never a condition of it, which is why this
/// type carries the digest that proves the bytes and nothing that could make
/// an installation refuse to proceed without one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexicalBridgeArtifact {
    bytes: Vec<u8>,
    sha256: String,
    source: String,
}

impl LexicalBridgeArtifact {
    /// Bytes whose digest the caller has already compared against a
    /// published checksum, or computed from a file an operator supplied.
    pub fn verified(bytes: Vec<u8>, sha256: String, source: String) -> Self {
        Self {
            bytes,
            sha256,
            source,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Where these bytes came from, for a receipt a human reads: a release
    /// asset name, or the path an operator named.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// A table of no bytes is a publishing accident, not a table.
    pub fn require_content(&self) -> Result<(), LifecycleError> {
        if self.is_empty() {
            return Err(LifecycleError::Network(format!(
                "lexical bridge table from {} is empty",
                self.source
            )));
        }
        Ok(())
    }
}
