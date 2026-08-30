use super::lifecycle_finding::LifecycleFinding;

/// One titled block of a rendered report: what a single observation of the
/// machine found, ready to be written in any style without looking again.
///
/// Rendering twice used to mean observing twice, and anything writing to a
/// store between the two renders made "styled equals plain" fail on a size
/// difference that had nothing to do with styling (#416).
#[derive(Clone, Debug)]
pub struct ReportSection {
    pub title: &'static str,
    pub findings: Vec<LifecycleFinding>,
}

impl ReportSection {
    pub fn new(title: &'static str, findings: Vec<LifecycleFinding>) -> Self {
        Self { title, findings }
    }

    pub fn single(title: &'static str, finding: LifecycleFinding) -> Self {
        Self::new(title, vec![finding])
    }
}
