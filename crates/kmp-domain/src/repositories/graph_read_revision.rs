/// An opaque identity for a complete, immutable graph and detail snapshot.
/// Equal identities from one reader must mean equal contents, including peer
/// writes, labels, lifecycle and bodies. This is not an aggregate sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphReadRevision(String);

impl GraphReadRevision {
    /// Opaque snapshot identity, never an authorization capability.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn new(identity: impl Into<String>) -> Result<Self, crate::DomainError> {
        let identity = identity.into();
        if identity.is_empty() {
            return Err(crate::DomainError::EmptyValue("graph read revision"));
        }
        Ok(Self(identity))
    }
}
