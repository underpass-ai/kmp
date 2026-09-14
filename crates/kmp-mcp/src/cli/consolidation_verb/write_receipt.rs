use kmp_domain::consolidation::ConsolidatedView;
use kmp_proto_mapping::consolidation_projection::ViewRevision;
use serde::Serialize;

/// Stable acceptance of one immutable revision. This receipt does not assert
/// that its sources or view head are still current when an operation is retried.
#[derive(Serialize)]
pub(super) struct WriteReceipt<'a> {
    status: &'static str,
    about: &'a str,
    view: &'a str,
    revision: u64,
    authored_at: &'a str,
    author: &'a str,
    source_count: usize,
    claim_count: usize,
    expansion: ViewRevision,
}

impl<'a> From<&'a ConsolidatedView> for WriteReceipt<'a> {
    fn from(view: &'a ConsolidatedView) -> Self {
        Self {
            status: "accepted",
            about: &view.about,
            view: &view.view,
            revision: view.revision,
            authored_at: &view.authored_at,
            author: &view.author,
            source_count: view.sources.len(),
            claim_count: view.claims.len(),
            expansion: ViewRevision {
                about: view.about.clone(),
                view: view.view.clone(),
                revision: view.revision,
            },
        }
    }
}
