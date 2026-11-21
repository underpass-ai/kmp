//! Whether `tools/list` advertises each tool's `outputSchema`.
//!
//! The schemas are the contract of `structuredContent` and stay in code; what
//! changes is only whether the catalogue carries them. Off by default: every
//! session pays the catalogue at startup, and the output schemas were 43 % of
//! it. A host that validates structured output opts in.
use serde_json::Value;

use super::KernelMcpServer;
use crate::contract::without_output_schemas;

/// On advertises output schemas in `tools/list`; unset, blank or off omits
/// them. It reads the spelling set every on/off switch reads.
pub const OUTPUT_SCHEMAS_ENV: &str = "KMP_MCP_OUTPUT_SCHEMAS";

fn output_schemas_enabled(value: Option<&str>) -> Result<bool, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(false);
    };
    crate::environment::on_off::parse_on_off(value).ok_or_else(|| {
        format!(
            "{OUTPUT_SCHEMAS_ENV}=`{value}` is not a value kmp-mcp accepts; it accepts {}",
            crate::environment::on_off::ON_OFF_ACCEPTED
        )
    })
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
                return Err(format!(
                    "{OUTPUT_SCHEMAS_ENV} is not UTF-8; it accepts {}",
                    crate::environment::on_off::ON_OFF_ACCEPTED
                ));
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
    fn the_on_off_words_are_accepted_and_blank_is_unset() {
        assert_eq!(output_schemas_enabled(None), Ok(false));
        assert_eq!(output_schemas_enabled(Some("")), Ok(false));
        assert_eq!(output_schemas_enabled(Some("0")), Ok(false));
        assert_eq!(output_schemas_enabled(Some("1")), Ok(true));
        assert_eq!(output_schemas_enabled(Some("True")), Ok(true));
        assert_eq!(output_schemas_enabled(Some("OFF")), Ok(false));
        let refused = output_schemas_enabled(Some("banana")).expect_err("refused");
        assert!(refused.contains("on, off"), "{refused}");
    }
}
