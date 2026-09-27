/// Whether a focused review proposes write-time lifecycle pairs — a new fact
/// and a current one of its about that share the principal anchor and entry
/// kind (DESIGN L4 4e) — and how. Off unless the store asks: measured on the
/// judged corpora, sharing an anchor was a replacement in none of the 68
/// pairs the rule proposed (P9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LifecycleMode {
    #[default]
    Off,
    /// Proposed as `supersedes` for the writer to confirm, retype or ignore.
    Rule,
    /// Read by Jev first; the pairs it reads as novel are withdrawn.
    Jev,
}

impl LifecycleMode {
    /// The mode a store names (`off`, `rule`, `jev`), or `None` for any
    /// other name.
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "off" => Some(Self::Off),
            "rule" => Some(Self::Rule),
            "jev" => Some(Self::Jev),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_names_one_of_three_modes_and_off_is_the_default() {
        assert_eq!(LifecycleMode::default(), LifecycleMode::Off);
        assert_eq!(LifecycleMode::named("rule"), Some(LifecycleMode::Rule));
        assert_eq!(LifecycleMode::named("jev"), Some(LifecycleMode::Jev));
        assert_eq!(LifecycleMode::named("off"), Some(LifecycleMode::Off));
        assert_eq!(LifecycleMode::named("always"), None);
    }
}
