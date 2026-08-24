//! Where the viewer serves, decided once.
//!
//! The binary mounts the viewer and the diagnostics tell an operator where to
//! look. Those two must not be able to disagree, so both read the answer from
//! here rather than each parsing the variable their own way.

use kmp_viewer::{DEFAULT_VIEWER_ADDR, VIEWER_ADDR_ENV};

/// How the viewer's address was chosen. The distinction is the whole point:
/// an address the operator asked for must be honoured or fail loudly, because
/// a typo that silently serves nothing is worse than a refusal. An address the
/// binary chose on their behalf must never cost them their memory, because a
/// port is not worth a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewerAddr {
    /// The operator turned the viewer off.
    Off,
    /// The operator named this address.
    Explicit(String),
    /// Nobody said anything, so the binary offers the documented default.
    Default(&'static str),
}

impl ViewerAddr {
    /// The address to bind, or `None` when the operator declined.
    pub fn addr(&self) -> Option<&str> {
        match self {
            Self::Off => None,
            Self::Explicit(addr) => Some(addr.as_str()),
            Self::Default(addr) => Some(addr),
        }
    }

    /// Whether a failure to bind should end the session. Only a request that
    /// was actually made can be refused.
    pub fn was_asked_for(&self) -> bool {
        matches!(self, Self::Explicit(_))
    }
}

/// Decides what an operator meant, given whatever `KMP_VIEWER_ADDR` held.
/// Absent means the default, not silence: the viewer is the only way a human
/// sees what the kernel is holding, and it shipped inside every binary while
/// reachable by nobody. An empty value, `off` or `none` is how one declines it.
pub fn classify_viewer_addr(raw: Option<&str>) -> ViewerAddr {
    let Some(raw) = raw else {
        return ViewerAddr::Default(DEFAULT_VIEWER_ADDR);
    };
    let value = raw.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("off") || value.eq_ignore_ascii_case("none") {
        return ViewerAddr::Off;
    }
    ViewerAddr::Explicit(value.to_string())
}

/// Reads `KMP_VIEWER_ADDR` from the process environment.
pub fn viewer_addr_from_env() -> ViewerAddr {
    classify_viewer_addr(std::env::var(VIEWER_ADDR_ENV).ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_variable_offers_the_documented_default() {
        assert_eq!(
            classify_viewer_addr(None),
            ViewerAddr::Default(DEFAULT_VIEWER_ADDR)
        );
    }

    #[test]
    fn the_default_is_the_address_the_documentation_already_promises() {
        // crates/kmp-viewer/README.md called this the default while nothing
        // fell back to it. The constant is the contract; keep them one thing.
        assert_eq!(DEFAULT_VIEWER_ADDR, "127.0.0.1:7317");
    }

    #[test]
    fn an_operator_declines_with_off_none_or_an_empty_value() {
        for declined in ["", "   ", "off", "OFF", "none", "None"] {
            assert_eq!(
                classify_viewer_addr(Some(declined)),
                ViewerAddr::Off,
                "`{declined}` should turn the viewer off"
            );
            assert_eq!(classify_viewer_addr(Some(declined)).addr(), None);
        }
    }

    #[test]
    fn a_named_address_is_explicit_and_trimmed() {
        let addr = classify_viewer_addr(Some("  127.0.0.1:9000 "));
        assert_eq!(addr, ViewerAddr::Explicit("127.0.0.1:9000".to_string()));
        assert_eq!(addr.addr(), Some("127.0.0.1:9000"));
        assert!(addr.was_asked_for());
    }

    #[test]
    fn a_default_is_distinguishable_from_the_same_address_named_out_loud() {
        // The two behave differently when the port is busy: one warns and
        // carries on, the other refuses. They must not compare equal.
        assert_ne!(
            classify_viewer_addr(None),
            classify_viewer_addr(Some(DEFAULT_VIEWER_ADDR))
        );
        assert!(!classify_viewer_addr(None).was_asked_for());
    }
}
