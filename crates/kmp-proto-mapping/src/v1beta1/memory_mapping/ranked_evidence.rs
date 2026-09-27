use kmp_proto::v1beta1::MemoryEvidence;

/// How many of the best eligible candidates make an ask's head (P14): the
/// window novelty reorders, repeated claims move to the end of, and every
/// rescue walks from. It is the diversification window.
pub(super) const HEAD_WINDOW: usize = crate::v1beta1::recall_projection::RANK_HEAD_WINDOW;

/// One ranking of an ask's candidates (P14, `kmp2` lazy pages).
///
/// `evidence[..head]` is the head: the best [`HEAD_WINDOW`] eligible
/// candidates, diversified and with repeated claims at its end, followed by
/// every rescue walked from them (restatements, lifecycle heads, expansions,
/// the reach graph, associations, the bridge). What follows is the tail: the
/// other eligible candidates in rank order. The head does not depend on the
/// tail, and any prefix of the tail is the tail of a deeper reading, so a
/// reading carried to any depth is a prefix of the exhaustive one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct RankedEvidence {
    pub(super) evidence: Vec<MemoryEvidence>,
    pub(super) head: usize,
}

impl RankedEvidence {
    /// A ranking with no tail: all of it is head.
    pub(super) fn whole(evidence: Vec<MemoryEvidence>) -> Self {
        let head = evidence.len();
        Self { evidence, head }
    }

    /// The same ranking carried to `depth` tail items at most, and whether
    /// the tail held more.
    pub(super) fn carried_to(mut self, depth: Option<usize>) -> (Self, bool) {
        let Some(depth) = depth else {
            return (self, false);
        };
        let end = self.head.saturating_add(depth);
        let more = self.evidence.len() > end;
        self.evidence.truncate(end);
        (self, more)
    }

    /// Keeps the items `keep` accepts, the head still the head.
    pub(super) fn retain(self, mut keep: impl FnMut(&MemoryEvidence) -> bool) -> Self {
        let mut head = 0;
        let mut evidence = Vec::with_capacity(self.evidence.len());
        for (index, item) in self.evidence.into_iter().enumerate() {
            if keep(&item) {
                head += usize::from(index < self.head);
                evidence.push(item);
            }
        }
        Self { evidence, head }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.to_string(),
            ..MemoryEvidence::default()
        }
    }

    #[test]
    fn a_deeper_reading_extends_a_shallower_one() {
        let ranked = RankedEvidence {
            evidence: ["h1", "h2", "t1", "t2", "t3"].map(item).to_vec(),
            head: 2,
        };
        let (shallow, more) = ranked.clone().carried_to(Some(1));
        assert!(more);
        assert_eq!(shallow.evidence.len(), 3);
        let (deep, more) = ranked.clone().carried_to(Some(3));
        assert!(!more);
        assert_eq!(&deep.evidence[..3], &shallow.evidence[..]);
        let kept = ranked.retain(|item| item.id != "h1");
        assert_eq!(kept.head, 1);
        assert_eq!(kept.evidence.len(), 4);
    }
}
