use std::sync::{Arc, Mutex};

/// What the rankings of one indexed read took through the lexical bridge
/// (DESIGN L6, P14): for each ranking, the direct score of the lowest
/// candidate its bridged rescue took, when it took as many as it may.
///
/// A ranking rescues, among the candidates it rejected, at most a few that
/// carry a word the table bridged the question to, the best by direct score
/// first. Top-k over the lexical index reads only the best of those carriers
/// by the score their rows give; [`Self::rank_above`] says, from what the
/// rankings took, whether a carrier left unread could have been taken.
///
/// A record, not part of what the ask stands on: two of them always compare
/// equal, and a clone shares what was recorded.
#[derive(Clone, Default)]
pub struct BridgedRescues {
    rankings: Arc<Mutex<Vec<Option<i64>>>>,
}

impl BridgedRescues {
    /// Records one ranking's bridged rescue: the direct scores of what it
    /// took, and how many it may take.
    pub(super) fn record(&self, taken: &[i64], cap: usize) {
        let lowest = taken.iter().copied().min().filter(|_| taken.len() >= cap);
        if let Ok(mut rankings) = self.rankings.lock() {
            rankings.push(lowest);
        }
    }

    /// Whether no carrier whose direct score is at most `unread` could have
    /// been rescued: some ranking ran, and every ranking took as many as it
    /// may, each above `unread`. Ties are not decided here (the rescue
    /// breaks them by a key the rows do not hold), so a tie is no.
    pub fn rank_above(&self, unread: i64) -> bool {
        self.rankings.lock().is_ok_and(|rankings| {
            !rankings.is_empty()
                && rankings
                    .iter()
                    .all(|lowest| lowest.is_some_and(|lowest| lowest > unread))
        })
    }
}

impl PartialEq for BridgedRescues {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for BridgedRescues {}

impl std::fmt::Debug for BridgedRescues {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BridgedRescues")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_rescue_above_the_unread_carrier_is_certain() {
        let rescues = BridgedRescues::default();
        assert!(!rescues.rank_above(0), "nothing ranked: nothing is known");
        rescues.record(&[90, 70, 50], 3);
        assert!(rescues.rank_above(49));
        assert!(
            !rescues.rank_above(50),
            "a tie is decided by a key rows lack"
        );
        rescues.record(&[80, 60], 3);
        assert!(
            !rescues.rank_above(10),
            "a short rescue may take any carrier"
        );
    }

    #[test]
    fn a_clone_shares_what_was_recorded_and_compares_equal() {
        let rescues = BridgedRescues::default();
        let shared = rescues.clone();
        shared.record(&[9, 8, 7], 3);
        assert!(rescues.rank_above(6));
        assert_eq!(rescues, BridgedRescues::default());
    }
}
