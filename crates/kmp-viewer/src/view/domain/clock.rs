//! Which clock the loom's axis reads.

/// KMP has no single clock, so the view has to say which one it means. The
/// vocabulary is closed: a string that is not one of these never becomes a
/// `Clock`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clock {
    /// When it happened in the world — the default axis.
    #[default]
    Occurred,
    /// When it was seen.
    Observed,
    /// When KMP persisted it.
    Ingested,
    /// For how long it held.
    Validity,
}

impl Clock {
    /// Every clock the kernel keeps, in the order the surfaces advertise.
    pub const ALL: [Clock; 4] = [
        Clock::Occurred,
        Clock::Observed,
        Clock::Ingested,
        Clock::Validity,
    ];

    /// The advertised names, for schemas and error prose.
    pub const NAMES: [&'static str; 4] = ["occurred", "observed", "ingested", "validity"];

    /// Parses an advertised name; anything else is not a clock.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "occurred" => Some(Clock::Occurred),
            "observed" => Some(Clock::Observed),
            "ingested" => Some(Clock::Ingested),
            "validity" => Some(Clock::Validity),
            _ => None,
        }
    }

    /// The advertised name of this clock.
    pub fn as_str(self) -> &'static str {
        match self {
            Clock::Occurred => "occurred",
            Clock::Observed => "observed",
            Clock::Ingested => "ingested",
            Clock::Validity => "validity",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_clock_parses_back_to_itself() {
        for (clock, name) in Clock::ALL.iter().zip(Clock::NAMES) {
            assert_eq!(Clock::parse(name), Some(*clock));
            assert_eq!(clock.as_str(), name);
        }
        assert_eq!(Clock::parse("vibes"), None);
        assert_eq!(Clock::default(), Clock::Occurred);
    }
}
