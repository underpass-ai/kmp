use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;
use sha2::{Digest, Sha256};

use super::scalars::{ProtoMappingResult, invalid_argument};

/// Evidence a remote judge found relevant to a stated intent, best first,
/// named by entry ref and the SHA-256 of its exact text. A focused wake keeps
/// these and withholds the rest; nothing here is text or proof.
#[derive(Debug, Clone)]
pub struct JudgedSelection {
    model: String,
    fingerprints: Vec<(String, String)>,
}

impl JudgedSelection {
    pub fn new(model: String, fingerprints: Vec<(String, String)>) -> ProtoMappingResult<Self> {
        let mut identities = BTreeSet::new();
        let valid = !model.trim().is_empty()
            && model.len() <= 256
            && fingerprints.len() <= 400
            && fingerprints.iter().all(|(entry_ref, fingerprint)| {
                !entry_ref.is_empty()
                    && entry_ref.len() <= 4096
                    && fingerprint.len() == 64
                    && fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
                    && identities.insert((entry_ref.clone(), fingerprint.to_ascii_lowercase()))
            });
        if !valid {
            return Err(invalid_argument("invalid judged selection"));
        }
        Ok(Self {
            model,
            fingerprints: fingerprints
                .into_iter()
                .map(|(entry_ref, hash)| (entry_ref, hash.to_ascii_lowercase()))
                .collect(),
        })
    }

    /// Where an evidence item stands in the judged order, if it was kept.
    pub(super) fn position(&self, item: &MemoryEvidence) -> Option<usize> {
        let fingerprint = format!("{:x}", Sha256::digest(item.text.as_bytes()));
        item.supports
            .iter()
            .filter_map(|entry_ref| {
                self.fingerprints
                    .iter()
                    .position(|(kept, hash)| kept == entry_ref && *hash == fingerprint)
            })
            .min()
    }

    pub(super) fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(supports: &str, text: &str) -> MemoryEvidence {
        MemoryEvidence {
            supports: vec![supports.into()],
            text: text.into(),
            ..MemoryEvidence::default()
        }
    }

    fn sha(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    #[test]
    fn only_the_judged_identity_is_kept_in_its_order() {
        let selection = JudgedSelection::new(
            "jev-1.13.0".into(),
            vec![("a:2".into(), sha("two")), ("a:1".into(), sha("one"))],
        )
        .expect("selection");
        assert_eq!(selection.position(&evidence("a:1", "one")), Some(1));
        assert_eq!(selection.position(&evidence("a:2", "two")), Some(0));
        assert_eq!(selection.position(&evidence("a:1", "changed")), None);
        assert_eq!(selection.position(&evidence("a:3", "three")), None);
    }

    #[test]
    fn duplicates_and_bad_fingerprints_are_refused() {
        assert!(
            JudgedSelection::new(
                "m".into(),
                vec![("a".into(), sha("x")), ("a".into(), sha("x"))]
            )
            .is_err()
        );
        assert!(JudgedSelection::new("m".into(), vec![("a".into(), "short".into())]).is_err());
    }
}
