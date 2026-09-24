//! Whether `tools/list` advertises each tool's `outputSchema`.
//!
//! The schemas are the contract of `structuredContent` and stay in code; what
//! changes is only whether the catalogue carries them. Off by default: every
//! session pays the catalogue at startup, and the output schemas were 43 % of
//! it. A host that validates structured output opts in.
use serde_json::Value;

use super::KernelMcpServer;
use crate::contract::without_output_schemas;

/// `1` advertises output schemas in `tools/list`; unset or `0` omits them.
pub const OUTPUT_SCHEMAS_ENV: &str = "KMP_MCP_OUTPUT_SCHEMAS";

fn output_schemas_enabled(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(format!("{OUTPUT_SCHEMAS_ENV} must be 0 or 1")),
    }
}

impl KernelMcpServer {
    /// Opt in at the host boundary. Tool calls answer the same either way;
    /// only the advertised catalogue gains the output schemas.
    pub fn with_output_schemas(mut self, enabled: bool) -> Self {
        self.output_schemas = enabled;
        self
    }

    /// Compose the same choice in stdio (with or without viewer) and HTTP startup.
    pub fn with_output_schemas_from_env(self) -> Result<Self, String> {
        let value = match std::env::var(OUTPUT_SCHEMAS_ENV) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(format!("{OUTPUT_SCHEMAS_ENV} must be 0 or 1"));
            }
        };
        let enabled = output_schemas_enabled(value.as_deref())?;
        Ok(self.with_output_schemas(enabled))
    }

    /// Applied last, after any projection that patches the schemas.
    pub(super) fn output_schema_tools(&self, result: Value) -> Value {
        if self.output_schemas {
            result
        } else {
            without_output_schemas(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::output_schemas_enabled;

    #[test]
    fn only_zero_one_or_unset_are_accepted() {
        assert_eq!(output_schemas_enabled(None), Ok(false));
        assert_eq!(output_schemas_enabled(Some("0")), Ok(false));
        assert_eq!(output_schemas_enabled(Some("1")), Ok(true));
        assert!(output_schemas_enabled(Some("true")).is_err());
        assert!(output_schemas_enabled(Some("")).is_err());
    }
}
