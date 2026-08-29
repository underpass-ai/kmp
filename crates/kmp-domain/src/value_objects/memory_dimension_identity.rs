use crate::DomainError;

const PREFIX: &str = "about:";
const SEPARATOR: &str = ":dimension:";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemoryDimensionIdentity {
    about: String,
    dimension_id: String,
}

impl MemoryDimensionIdentity {
    pub fn new(
        about: impl Into<String>,
        dimension_id: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let about = normalize_required(about.into(), "memory dimension about")?;
        let dimension_id = normalize_required(dimension_id.into(), "memory dimension id")?;
        Ok(Self {
            about,
            dimension_id,
        })
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().strip_prefix(PREFIX)?;
        let (about, dimension_id) = value.split_once(SEPARATOR)?;
        Self::new(about, dimension_id).ok()
    }

    /// Resolve `value` against `about`, whether it arrives bare or already
    /// namespaced. Reads hand out the namespaced form, so both forms reach
    /// writers; wrapping an already-namespaced id a second time would name a
    /// different dimension that reads back as the intended one. An id
    /// namespaced for another about is not this about's to reinterpret.
    pub fn resolve(about: &str, value: &str) -> Option<Self> {
        match Self::parse(value) {
            Some(identity) if identity.about() == about => Some(identity),
            Some(_) => None,
            None => Self::new(about, value).ok(),
        }
    }

    pub fn node_id(&self) -> String {
        format!("{PREFIX}{}{SEPARATOR}{}", self.about, self.dimension_id)
    }

    pub fn about(&self) -> &str {
        &self.about
    }

    pub fn dimension_id(&self) -> &str {
        &self.dimension_id
    }
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
    fn identity_formats_and_parses_namespaced_dimension_node_id() {
        let identity =
            MemoryDimensionIdentity::new("question:830ce83f", "timeline").expect("valid identity");

        assert_eq!(
            identity.node_id(),
            "about:question:830ce83f:dimension:timeline"
        );
        let parsed = MemoryDimensionIdentity::parse(&identity.node_id()).expect("should parse");
        assert_eq!(parsed.about(), "question:830ce83f");
        assert_eq!(parsed.dimension_id(), "timeline");
    }

    #[test]
    fn resolve_is_idempotent_for_this_about() {
        let bare = MemoryDimensionIdentity::resolve("project:x", "run").expect("bare id resolves");
        assert_eq!(bare.node_id(), "about:project:x:dimension:run");

        let namespaced = MemoryDimensionIdentity::resolve("project:x", &bare.node_id())
            .expect("namespaced id resolves");
        assert_eq!(namespaced.node_id(), bare.node_id());
        assert_eq!(namespaced.dimension_id(), "run");
    }

    #[test]
    fn resolve_refuses_a_dimension_owned_by_another_about() {
        assert!(
            MemoryDimensionIdentity::resolve("project:x", "about:project:y:dimension:run")
                .is_none()
        );
    }

    #[test]
    fn identity_rejects_empty_parts() {
        assert!(MemoryDimensionIdentity::new("", "timeline").is_err());
        assert!(MemoryDimensionIdentity::new("question:830ce83f", " ").is_err());
    }
}
