use std::collections::BTreeMap;

/// How often each concept occurs in one text, and how long that text is.
///
/// Counting is what BM25 needs and what the ranker's term sets threw away.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct TermCounts {
    counts: BTreeMap<String, u32>,
    length: usize,
}

impl TermCounts {
    pub(super) fn insert(&mut self, term: String) {
        *self.counts.entry(term).or_default() += 1;
        self.length += 1;
    }

    pub(super) fn terms(&self) -> impl Iterator<Item = &String> {
        self.counts.keys()
    }

    pub(super) fn count(&self, term: &str) -> u32 {
        self.counts.get(term).copied().unwrap_or(0)
    }

    pub(super) fn length(&self) -> usize {
        self.length
    }
}

impl FromIterator<String> for TermCounts {
    fn from_iter<I: IntoIterator<Item = String>>(terms: I) -> Self {
        let mut counts = Self::default();
        for term in terms {
            counts.insert(term);
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(terms: &[&str]) -> TermCounts {
        terms.iter().map(|term| (*term).to_string()).collect()
    }

    #[test]
    fn counting_preserves_frequency_and_length() {
        let bag = counts(&["valkey", "store", "valkey"]);

        assert_eq!(bag.count("valkey"), 2);
        assert_eq!(bag.count("store"), 1);
        assert_eq!(bag.count("absent"), 0);
        assert_eq!(bag.length(), 3);
        assert_eq!(TermCounts::default().length(), 0);
    }
}
