//! Whether an agent enters KMP on its own, or only when it is asked to.
//!
//! Two states, one consequence: which sentence the MCP initialize
//! instructions open with. The default is the conservative one. Having KMP
//! installed is not the same as asking for it, and an unbidden wake against
//! an empty or unrelated store costs a round trip and can shape an answer
//! with evidence nobody asked for.

/// Root config key that owns this setting.
pub const KEY: &str = "memory_routing";

const ON_REQUEST: &str = "on_request";
const ALWAYS: &str = "always";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryRouting {
    /// Call a kmp tool when the user, a kmp skill, or the project asks for
    /// memory. Otherwise the agent works from what is already in front of it.
    #[default]
    OnRequest,
    /// Enter known work through `kmp_wake` without being asked. Deliberate,
    /// never the default.
    Always,
}

impl MemoryRouting {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            ON_REQUEST => Ok(Self::OnRequest),
            ALWAYS => Ok(Self::Always),
            _ => Err(format!(
                "{value:?} is not a memory routing mode: use \"on-request\" or \"always\""
            )),
        }
    }

    /// The value as it is written to `config.toml`.
    pub fn config_value(self) -> &'static str {
        match self {
            Self::OnRequest => ON_REQUEST,
            Self::Always => ALWAYS,
        }
    }

    /// The value as a person reads it in `kmp-mcp config` and the doctor.
    pub fn label(self) -> &'static str {
        match self {
            Self::OnRequest => "on request",
            Self::Always => "always",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_written_forms_and_defaults_to_on_request() {
        assert_eq!(MemoryRouting::default(), MemoryRouting::OnRequest);
        for value in ["on_request", "on-request", "  ON-Request  "] {
            assert_eq!(
                MemoryRouting::parse(value).expect("supported mode"),
                MemoryRouting::OnRequest
            );
        }
        assert_eq!(
            MemoryRouting::parse("Always").expect("supported mode"),
            MemoryRouting::Always
        );
    }

    #[test]
    fn rejects_a_mode_it_does_not_implement() {
        let error = MemoryRouting::parse("sometimes").expect_err("unsupported mode");
        assert!(error.contains("\"sometimes\""));
        assert!(error.contains("on-request"));
    }

    #[test]
    fn round_trips_through_its_config_value() {
        for routing in [MemoryRouting::OnRequest, MemoryRouting::Always] {
            assert_eq!(
                MemoryRouting::parse(routing.config_value()).expect("its own value"),
                routing
            );
        }
        assert_eq!(MemoryRouting::OnRequest.label(), "on request");
        assert_eq!(MemoryRouting::Always.label(), "always");
    }
}
