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
    pub(crate) fn token(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"kmp.curate.review.v1\0");
        hasher.update(self.selection.as_bytes());
        hasher.update(format!("{:?}", self.findings).as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
