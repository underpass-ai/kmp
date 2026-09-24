use sha2::{Digest, Sha256};

use crate::curate::application::jev_usage::JevUsage;
use crate::curate::domain::curate_finding::CurateFinding;

/// A finished review. Its token binds the relate selection and every
/// finding, so a later page or apply reads exactly this review.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurateReview {
    pub findings: Vec<CurateFinding>,
    pub jev: Option<JevUsage>,
    pub warnings: Vec<String>,
    pub selection: String,
}

impl CurateReview {
    /// Every finding with the id the tool shows for it: `m<n>` counts the
    /// missing items and `s<n>` the suspect ones, each in review order. The
    /// DTO and apply both read ids from here, so they cannot disagree.
    pub(crate) fn numbered(&self) -> Vec<(String, &CurateFinding)> {
        let (mut missing, mut suspect) = (0, 0);
        self.findings
            .iter()
            .map(|finding| match finding {
                CurateFinding::Missing { .. } => {
                    missing += 1;
                    (format!("m{}", missing - 1), finding)
                }
                CurateFinding::Suspect { .. } => {
                    suspect += 1;
                    (format!("s{}", suspect - 1), finding)
                }
            })
            .collect()
    }

    pub(crate) fn token(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"kmp.curate.review.v1\0");
        hasher.update(self.selection.as_bytes());
        hasher.update(format!("{:?}", self.findings).as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
