use crate::DomainError;

const LABEL_PREFIX: &str = "label:v1:";

/// The identity of a dimensional label is its about, key and value.
/// References encode these components without exposing punctuation as syntax.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemoryDimensionIdentity {
    about: String,
    key: String,
    dimension_id: String,
}

impl MemoryDimensionIdentity {
    pub fn new(
        about: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            about: normalize_required(about.into(), "memory dimension about")?,
            key: normalize_required(key.into(), "memory dimension key")?,
            dimension_id: normalize_required(value.into(), "memory dimension value")?,
        })
    }

    pub fn parse(reference: &str) -> Option<Self> {
        let encoded = reference.strip_prefix(LABEL_PREFIX)?;
        let mut parts = encoded.split(':');
        let about = decode_component(parts.next()?)?;
        let key = decode_component(parts.next()?)?;
        let value = decode_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let identity = Self::new(about, key, value).ok()?;
        (identity.node_id() == reference).then_some(identity)
    }

    /// Resolve a complete ref inside its owning about. Bare values need a
    /// key and are constructed with `new`; old refs are not reinterpreted.
    pub fn resolve(about: &str, reference: &str) -> Option<Self> {
        Self::parse(reference).filter(|identity| identity.about() == about)
    }

    pub fn node_id(&self) -> String {
        format!(
            "{LABEL_PREFIX}{}:{}:{}",
            encode_component(&self.about),
            encode_component(&self.key),
            encode_component(&self.dimension_id)
        )
    }

    pub fn about(&self) -> &str {
        &self.about
    }

    pub fn dimension_id(&self) -> &str {
        &self.dimension_id
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

fn decode_component(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut encoded = value.bytes();
    while let Some(byte) = encoded.next() {
        if byte == b'%' {
            let high = char::from(encoded.next()?).to_digit(16)?;
            let low = char::from(encoded.next()?).to_digit(16)?;
            bytes.push(u8::try_from((high << 4) | low).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    // One ref spelling per label: reject malformed/non-canonical escapes.
    (encode_component(&decoded) == value).then_some(decoded)
}

fn normalize_required(value: String, field: &'static str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        Err(DomainError::EmptyValue(field))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryDimensionIdentity;

    #[test]
    fn identity_has_one_canonical_spelling_and_requires_an_owner() {
        let identity = MemoryDimensionIdentity::new("project:x", "alias", "neb").expect("identity");
        assert_eq!(identity.node_id(), "label:v1:project%3Ax:alias:neb");
        assert_eq!(
            MemoryDimensionIdentity::parse(&identity.node_id()),
            Some(identity.clone())
        );
        assert_eq!(
            MemoryDimensionIdentity::resolve("project:x", &identity.node_id()),
            Some(identity.clone())
        );
        assert!(MemoryDimensionIdentity::resolve("project:y", &identity.node_id()).is_none());
        assert!(MemoryDimensionIdentity::parse("about:project:x:dimension:neb").is_none());
    }

    #[test]
    fn labels_with_the_same_value_keep_distinct_keys_and_abouts() {
        let alias = MemoryDimensionIdentity::new("project:x", "alias", "neb").expect("alias");
        let component =
            MemoryDimensionIdentity::new("project:x", "component", "neb").expect("component");
        let foreign = MemoryDimensionIdentity::new("project:y", "alias", "neb").expect("foreign");
        assert_ne!(alias.node_id(), component.node_id());
        assert_ne!(alias.node_id(), foreign.node_id());
    }

    #[test]
    fn labels_round_trip_punctuation_and_unicode_without_ambiguous_separators() {
        let identity = MemoryDimensionIdentity::new(
            "project:x:dimension:a:label:b",
            "alias",
            "Nébula Cache / production:west",
        )
        .expect("label");
        let parsed = MemoryDimensionIdentity::parse(&identity.node_id()).expect("parse");
        assert_eq!(parsed, identity);
        assert_eq!(parsed.key(), "alias");
        assert_eq!(parsed.dimension_id(), "Nébula Cache / production:west");
        assert!(!identity.node_id().contains('/'));
    }

    #[test]
    fn malformed_refs_and_empty_components_are_rejected() {
        for value in [
            "label:v1:project:alias:%zz",
            "label:v1:project:alias:%FF",
            "label:v1:project:alias:neb:extra",
            "label:v1:project::neb",
            "label:v1:project:alias:bad value",
            "label:v1:project%3ax:alias:neb",
            "label:v1:project:alias:%20neb",
        ] {
            assert!(MemoryDimensionIdentity::parse(value).is_none(), "{value}");
        }
        for (about, key, value) in [
            ("", "alias", "neb"),
            ("project:x", "", "neb"),
            ("project:x", "alias", " "),
        ] {
            assert!(MemoryDimensionIdentity::new(about, key, value).is_err());
        }
    }
}
