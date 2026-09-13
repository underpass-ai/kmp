//! What a host's own report actually proves about the KMP MCP.
//!
//! Three facts were being collapsed into one word. "Installed and enabled" is
//! what a host manager's inventory can tell you; it says a registration
//! exists and is switched on. "The binary declares its tools" is what running
//! the engine proves; it says the executable answers the contract. Neither is
//! a connection. Only a host that reports the MCP connected has verified one,
//! and #680 is exactly the case where a registration was called usable while
//! nothing had ever connected.
//!
//! Where no live probe ran, the honest verdict is unverified — a warning a
//! reader can act on, not an approval nobody earned.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostConnectionVerification {
    /// The host reports a live MCP connection it made itself.
    Verified,
    /// The registration is installed and enabled, and no live connection has
    /// been observed from here.
    Unverified,
    /// The registration is missing, disabled, unapproved or failing.
    Broken,
}

impl HostConnectionVerification {
    /// The headline clause, in the words the doctor prints.
    pub fn headline(self) -> &'static str {
        match self {
            Self::Verified => "live MCP connection verified",
            Self::Unverified => {
                "MCP registration installed and enabled, live connection \
                                 unverified"
            }
            Self::Broken => "effective MCP registration is not usable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinction this type exists to keep: an unverified registration
    /// never reads as a working one.
    #[test]
    fn only_a_live_connection_is_reported_as_verified() {
        assert_eq!(
            HostConnectionVerification::Verified.headline(),
            "live MCP connection verified"
        );
        let unverified = HostConnectionVerification::Unverified.headline();
        assert!(unverified.contains("installed and enabled"), "{unverified}");
        assert!(unverified.contains("unverified"), "{unverified}");
        assert!(
            !unverified.contains("usable"),
            "an unverified registration must not claim usability: {unverified}"
        );
    }
}
