//! Why a value of an environment switch was refused.

use std::fmt;

/// An environment switch holding a value the engine does not accept. It is
/// refused at startup rather than read as the default: a typo that silently
/// changed nothing (`KMP_KERNEL_GRPC_TLS_MODE=mutal` ran plaintext) is the
/// failure this exists to end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchRefusal {
    name: &'static str,
    value: String,
    accepted: String,
    why: Option<String>,
}

impl SwitchRefusal {
    pub fn new(name: &'static str, value: impl Into<String>, accepted: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
            accepted: accepted.into(),
            why: None,
        }
    }

    /// The reason this particular value cannot be honoured.
    pub fn because(mut self, why: impl Into<String>) -> Self {
        let why: String = why.into();
        self.why = Some(why.trim_end_matches('.').to_string());
        self
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// What the switch accepts, as the refusal states it.
    pub fn accepted(&self) -> &str {
        &self.accepted
    }
}

impl fmt::Display for SwitchRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}=`{}` is not a value kmp-mcp accepts; it accepts {}.",
            self.name, self.value, self.accepted
        )?;
        if let Some(why) = &self.why {
            write!(formatter, " {why}.")?;
        }
        write!(formatter, " Fix it, or unset {} for its default.", self.name)
    }
}

impl std::error::Error for SwitchRefusal {}
