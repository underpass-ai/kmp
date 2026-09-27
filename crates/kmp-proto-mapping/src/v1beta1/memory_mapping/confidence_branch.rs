use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_selection::BRIDGED_TERMS_KEY;

/// Which road an answer's confidence came by, for its calibration.
///
/// `Bridged` wins over the other two: a citation that answered with the
/// lexical bridge's words is capped however the question was decided.
/// Otherwise an answer the anchored gate decided is `Anchored`, and the
/// ⌈2/3⌉ reading of a question without identifiers is `Unanchored`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConfidenceBranch {
    Anchored,
    Unanchored,
    Bridged,
}

impl ConfidenceBranch {
    pub(super) fn read(anchored: bool, retained: &[MemoryEvidence]) -> Self {
        if retained
            .iter()
            .any(|item| item.metadata.contains_key(BRIDGED_TERMS_KEY))
        {
            Self::Bridged
        } else if anchored {
            Self::Anchored
        } else {
            Self::Unanchored
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(bridged: bool) -> MemoryEvidence {
        let mut item = MemoryEvidence::default();
        if bridged {
            item.metadata
                .insert(BRIDGED_TERMS_KEY.to_string(), "fallo=failure".to_string());
        }
        item
    }

    #[test]
    fn a_bridged_citation_names_the_branch_whatever_decided() {
        assert_eq!(
            ConfidenceBranch::read(true, &[item(false), item(true)]),
            ConfidenceBranch::Bridged
        );
        assert_eq!(
            ConfidenceBranch::read(true, &[item(false)]),
            ConfidenceBranch::Anchored
        );
        assert_eq!(
            ConfidenceBranch::read(false, &[item(false)]),
            ConfidenceBranch::Unanchored
        );
    }
}
