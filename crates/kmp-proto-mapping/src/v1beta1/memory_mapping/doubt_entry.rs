/// Why an ask entered the doubt band (DESIGN L4 4f): the deterministic
/// reading settled, but in one of the places where the kernel's words are
/// weakest. Never entered when a required anchor is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubtEntry {
    /// (i) A question without a required anchor came back UNKNOWN while a
    /// `best_effort` reading of it would cite something.
    UnanchoredUnknown,
    /// (ii) The anchored gate found the anchor but not what was asked of it.
    AttributeNotFound,
    /// (ii) An enumerative question some of whose concepts no cited memory
    /// states beside the anchor.
    Partial,
    /// (iii) An answer whose first citation leads the second by less than
    /// the store's margin.
    NarrowMargin,
}

impl DoubtEntry {
    /// The word telemetry and warnings name the entry by; add, never rename.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnanchoredUnknown => "unanchored_unknown",
            Self::AttributeNotFound => "attribute_not_found",
            Self::Partial => "partial",
            Self::NarrowMargin => "narrow_margin",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DoubtEntry;

    #[test]
    fn every_entry_has_its_own_word() {
        let words = [
            DoubtEntry::UnanchoredUnknown,
            DoubtEntry::AttributeNotFound,
            DoubtEntry::Partial,
            DoubtEntry::NarrowMargin,
        ]
        .map(DoubtEntry::as_str);
        let unique = words.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), words.len());
    }
}
