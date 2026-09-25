use std::collections::VecDeque;
use std::sync::Mutex;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;

const KEPT: usize = 16;

/// Frozen reviews by token, so pages and apply read the review that was
/// shown instead of a new one. Oldest out first.
#[derive(Default)]
pub(crate) struct CurateReviewCache {
    reviews: Mutex<VecDeque<(String, CurateReview, CurateMaterial)>>,
}

impl CurateReviewCache {
    pub(crate) fn insert(&self, token: String, review: CurateReview, material: CurateMaterial) {
        let mut reviews = self
            .reviews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reviews.retain(|(kept, _, _)| *kept != token);
        if reviews.len() >= KEPT {
            reviews.pop_front();
        }
        reviews.push_back((token, review, material));
    }

    pub(crate) fn get(&self, token: &str) -> Option<(CurateReview, CurateMaterial)> {
        let reviews = self
            .reviews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reviews
            .iter()
            .find(|(kept, _, _)| kept == token)
            .map(|(_, review, material)| (review.clone(), material.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(selection: &str) -> (CurateReview, CurateMaterial) {
        (
            CurateReview {
                findings: vec![],
                jev: None,
                warnings: vec![],
                selection: selection.into(),
            },
            CurateMaterial {
                facts: vec![],
                declared: vec![],
                pairs: vec![],
                selection: selection.into(),
                past: Vec::new(),
            },
        )
    }

    #[test]
    fn the_oldest_review_leaves_first() {
        let cache = CurateReviewCache::default();
        for n in 0..17 {
            let (review, material) = entry(&n.to_string());
            cache.insert(format!("t{n}"), review, material);
        }
        assert!(cache.get("t0").is_none());
        assert_eq!(cache.get("t16").expect("kept").0.selection, "16");
    }
}
