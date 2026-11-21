//! One environment variable that changes what kmp-mcp does.

use super::switch_refusal::SwitchRefusal;

/// A switch: its name, what it accepts, its default and its effect, and the
/// check that refuses a value it would not honour. The check is the same
/// parse the reader of the switch uses, so the refusal at startup and the
/// value read later cannot disagree.
#[derive(Clone, Copy)]
pub struct EnvironmentSwitch {
    name: &'static str,
    default: &'static str,
    accepted: &'static str,
    effect: &'static str,
    accepts: fn(&str) -> bool,
    explain: Option<fn(&str) -> Option<String>>,
    secret: bool,
    hook: bool,
}

impl EnvironmentSwitch {
    pub(crate) const fn new(
        name: &'static str,
        default: &'static str,
        accepted: &'static str,
        effect: &'static str,
        accepts: fn(&str) -> bool,
    ) -> Self {
        Self {
            name,
            default,
            accepted,
            effect,
            accepts,
            explain: None,
            secret: false,
            hook: false,
        }
    }

    /// A switch whose refusal also says why this value cannot be honoured.
    pub(crate) const fn explained_by(mut self, explain: fn(&str) -> Option<String>) -> Self {
        self.explain = Some(explain);
        self
    }

    /// A switch whose value is never printed.
    pub(crate) const fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    /// A test or evaluation hook (only debug and `eval-hooks` builds).
    pub(crate) const fn hook(
        name: &'static str,
        default: &'static str,
        accepted: &'static str,
        effect: &'static str,
        accepts: fn(&str) -> bool,
    ) -> Self {
        let mut switch = Self::new(name, default, accepted, effect, accepts);
        switch.hook = true;
        switch
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn default_value(&self) -> &'static str {
        self.default
    }

    pub fn accepted(&self) -> &'static str {
        self.accepted
    }

    pub fn effect(&self) -> &'static str {
        self.effect
    }

    pub fn is_secret(&self) -> bool {
        self.secret
    }

    pub fn is_hook(&self) -> bool {
        self.hook
    }

    /// The value set in the environment, trimmed; `None` when unset or blank.
    /// A value that is not UTF-8 or that the switch does not accept is
    /// refused.
    pub fn read(&self) -> Result<Option<String>, SwitchRefusal> {
        match std::env::var(self.name) {
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(raw)) => Err(SwitchRefusal::new(
                self.name,
                raw.to_string_lossy(),
                self.accepted,
            )),
            Ok(raw) => {
                let value = raw.trim();
                if value.is_empty() {
                    return Ok(None);
                }
                if (self.accepts)(value) {
                    Ok(Some(value.to_string()))
                } else {
                    let refusal = SwitchRefusal::new(self.name, value, self.accepted);
                    Err(match self.explain.and_then(|explain| explain(value)) {
                        Some(why) => refusal.because(why),
                        None => refusal,
                    })
                }
            }
        }
    }
}
