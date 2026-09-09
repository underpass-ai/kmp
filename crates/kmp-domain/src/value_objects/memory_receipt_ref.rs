use super::reference_component::{decode_component, encode_component};
use crate::DomainError;

const PREFIX: &str = "receipt:v1:";

/// A command audit address. It is not a graph memory or a dimensional label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryReceiptRef {
    about: String,
    idempotency_key: String,
}

impl MemoryReceiptRef {
    pub fn new(
        about: impl Into<String>,
        idempotency_key: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let about = about.into();
        let idempotency_key = idempotency_key.into();
        if about.trim().is_empty() {
            return Err(DomainError::EmptyValue("receipt about"));
        }
        if idempotency_key.trim().is_empty() {
            return Err(DomainError::EmptyValue("receipt idempotency key"));
        }
        Ok(Self {
            about,
            idempotency_key,
        })
    }

    pub fn parse(reference: &str) -> Option<Self> {
        let mut parts = reference.strip_prefix(PREFIX)?.split(':');
        let about = decode_component(parts.next()?)?;
        let key = decode_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let receipt = Self::new(about, key).ok()?;
        (receipt.reference() == reference).then_some(receipt)
    }

    pub fn reference(&self) -> String {
        format!(
            "{PREFIX}{}:{}",
            encode_component(&self.about),
            encode_component(&self.idempotency_key)
        )
    }

    pub fn about(&self) -> &str {
        &self.about
    }
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_audit_address_preserves_its_owner_and_exact_logical_key() {
        let reference =
            MemoryReceiptRef::new("project:gateway", "  write:á/%  ").expect("reference");
        let parsed = MemoryReceiptRef::parse(&reference.reference()).expect("parse");
        assert_eq!(parsed, reference);
        assert_eq!(parsed.idempotency_key(), "  write:á/%  ");
        assert_eq!(parsed.about(), "project:gateway");
        assert_ne!(
            reference,
            MemoryReceiptRef::new("project:other", reference.idempotency_key()).expect("other")
        );
        for invalid in [
            "receipt:v1:project%3agateway:key",
            "receipt:v1:project:key:extra",
            "receipt:v1:project:",
            "receipt:v1:project:%FF",
        ] {
            assert!(MemoryReceiptRef::parse(invalid).is_none(), "{invalid}");
        }
    }
}
