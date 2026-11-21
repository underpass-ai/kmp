/// How code reads one yes/no probability from Jev: TypeSafe's starting
/// lines, yes at or above 0.7, no at or below 0.3, and between them a
/// question the state does not settle, which every site gives its own
/// outcome (the decision is code, never the model's).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoulReading {
    Yes,
    Unsettled,
    No,
}

impl NoulReading {
    /// At or above this, the answer reads yes.
    pub(crate) const YES_AT: f64 = 0.7;
    /// At or below this, the answer reads no.
    pub(crate) const NO_AT: f64 = 0.3;

    pub(crate) fn of(yes: f64) -> Self {
        if yes >= Self::YES_AT {
            Self::Yes
        } else if yes <= Self::NO_AT {
            Self::No
        } else {
            Self::Unsettled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NoulReading;

    #[test]
    fn the_band_between_the_lines_is_unsettled() {
        assert_eq!(NoulReading::of(0.7), NoulReading::Yes);
        assert_eq!(NoulReading::of(0.69), NoulReading::Unsettled);
        assert_eq!(NoulReading::of(0.31), NoulReading::Unsettled);
        assert_eq!(NoulReading::of(0.3), NoulReading::No);
    }
}
