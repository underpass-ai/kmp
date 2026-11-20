/// Why an argument value is outside the tool's published input schema.
///
/// `field` is the argument's path below the tool name (`memories[0].kind`),
/// empty for the call as a whole; `unknown` says the value is an argument
/// the schema does not declare; `allowed` lists the values an `enum` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchemaViolation {
    pub(crate) message: String,
    pub(crate) field: String,
    pub(crate) unknown: bool,
    pub(crate) allowed: Vec<serde_json::Value>,
}

impl SchemaViolation {
    pub(crate) fn at(path: &str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: field_of(path),
            unknown: false,
            allowed: Vec::new(),
        }
    }

    pub(crate) fn allowing(mut self, allowed: &[serde_json::Value]) -> Self {
        self.allowed = allowed.to_vec();
        self
    }

    pub(crate) fn unknown_argument(path: &str, key: &str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: field_of(&format!("{path}.{key}")),
            unknown: true,
            allowed: Vec::new(),
        }
    }
}

/// `kmp_write_memory.memories[0].kind` → `memories[0].kind`.
fn field_of(path: &str) -> String {
    match path.find(['.', '[']) {
        Some(index) => path[index..].trim_start_matches('.').to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_drops_the_tool_name() {
        assert_eq!(SchemaViolation::at("kmp_time", "x").field, "");
        assert_eq!(SchemaViolation::at("kmp_time.page", "x").field, "page");
        assert_eq!(
            SchemaViolation::at("kmp_write_memory.memories[0].kind", "x").field,
            "memories[0].kind"
        );
        let unknown = SchemaViolation::unknown_argument("kmp_wake", "zz", "x");
        assert!(unknown.unknown);
        assert_eq!(unknown.field, "zz");
    }
}
