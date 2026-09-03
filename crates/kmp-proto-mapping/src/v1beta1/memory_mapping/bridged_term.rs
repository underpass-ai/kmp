/// One word of the question and the word of a candidate the table says means
/// the same thing, with how sure the table is.
///
/// This is the unit of provenance for a bridged hit. A reader auditing an
/// answer sees `valvula≈valve 0.51`, not a sentence score, and can decide
/// whether to trust the pair.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BridgedTerm {
    /// As the reader wrote it, folded.
    pub(super) question: String,
    /// As the memory wrote it, folded.
    pub(super) candidate: String,
    pub(super) similarity: f64,
}

impl BridgedTerm {
    /// `valvula≈valve 0.51`: the pair and the table's opinion of it, in the
    /// form a reader audits. Two decimals are the resolution the bar is set
    /// at; more would print float noise as if it were information.
    pub(super) fn describe(&self) -> String {
        format!(
            "{}≈{} {:.2}",
            self.question, self.candidate, self.similarity
        )
    }

    /// Every pair, `; `-separated, in the order they were bridged.
    pub(super) fn describe_all<'a>(pairs: impl IntoIterator<Item = &'a BridgedTerm>) -> String {
        pairs
            .into_iter()
            .map(BridgedTerm::describe)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_describes_itself_for_an_auditor() {
        let pair = BridgedTerm {
            question: "valvula".to_string(),
            candidate: "valve".to_string(),
            similarity: 0.5123,
        };
        let other = BridgedTerm {
            question: "noche".to_string(),
            candidate: "night".to_string(),
            similarity: 0.9,
        };

        assert_eq!(pair.describe(), "valvula≈valve 0.51");
        assert_eq!(
            BridgedTerm::describe_all([&pair, &other]),
            "valvula≈valve 0.51; noche≈night 0.90"
        );
        assert_eq!(BridgedTerm::describe_all([]), "");
    }
}
