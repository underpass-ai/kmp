use kmp_domain::GraphReadRevision;
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto::v1beta1::RecallProjection;
use serde_json::Value;

use crate::serving::frozen_recall::FrozenRecall;
use crate::serving::frozen_recall_key::FrozenRecallKey;
use crate::serving::ports::frozen_recall_store::FrozenRecallStore;

/// Continuations of Wake and Ask cut from the first page's frozen read.
///
/// Only a call carrying a cursor may thaw a read, and only at the store's
/// current revision — or the one restart a first page with a shortened core
/// proposed, which is the same query at a larger byte ceiling. Any other
/// call reads as it always has. The kernel's
/// read already stands on the memory frontier and never on the wall clock,
/// so an unchanged revision reads the same selection, lifecycle and proof.
#[derive(Clone, Copy)]
pub(crate) struct FrozenRecallReads<'a> {
    store: &'a dyn FrozenRecallStore,
    service: &'a EmbeddedMemoryService,
}

impl<'a> FrozenRecallReads<'a> {
    pub(crate) fn new(
        store: &'a dyn FrozenRecallStore,
        service: &'a EmbeddedMemoryService,
    ) -> Self {
        Self { store, service }
    }

    /// The first page's read when this call continues it and nothing was
    /// committed since; `None` sends the call down the ordinary read.
    pub(crate) async fn thaw(
        &self,
        key: &FrozenRecallKey,
        arguments: &Value,
    ) -> Option<FrozenRecall> {
        // A store that cannot certify its revision is read again; an error
        // here surfaces, if it persists, from the ordinary read.
        let revision = self.service.read_revision().await.ok().flatten()?;
        if continues(arguments) {
            self.store.thaw(key, &revision)
        } else {
            self.store.thaw_restart(key, &revision)
        }
    }

    /// Keep a read whose projected page left more to page through, or whose
    /// shortened core proposed a restart.
    pub(crate) fn freeze(
        &self,
        key: FrozenRecallKey,
        revision: Option<GraphReadRevision>,
        recall: FrozenRecall,
        projection: Option<&RecallProjection>,
    ) {
        let restart = restarts(projection);
        if let Some(revision) = revision
            && (has_more(projection) || restart)
        {
            self.store.freeze(key, revision, recall, restart);
        }
    }
}

fn continues(arguments: &Value) -> bool {
    arguments
        .pointer("/page/cursor")
        .and_then(Value::as_str)
        .is_some_and(|cursor| !cursor.is_empty())
}

/// The page shortened its core, so its next action restarts the read.
fn restarts(projection: Option<&RecallProjection>) -> bool {
    projection.is_some_and(|projection| projection.core_text_shortened)
}

fn has_more(projection: Option<&RecallProjection>) -> bool {
    projection
        .and_then(|projection| projection.page.as_ref())
        .is_some_and(|page| page.has_more && page.next_cursor.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::RecallProjectionPage;
    use serde_json::json;

    #[test]
    fn only_a_cursor_continues_a_read() {
        assert!(continues(
            &json!({"about": "a", "page": {"cursor": "kmp1:3:x"}})
        ));
        assert!(!continues(&json!({"about": "a", "page": {"cursor": ""}})));
        assert!(!continues(&json!({"about": "a", "page": {"entries": 3}})));
        assert!(!continues(&json!({"about": "a"})));
    }

    #[test]
    fn only_a_page_with_a_next_cursor_is_worth_keeping() {
        let page = |has_more, next_cursor: Option<&str>| RecallProjection {
            page: Some(RecallProjectionPage {
                has_more,
                next_cursor: next_cursor.map(str::to_string),
                ..RecallProjectionPage::default()
            }),
            ..RecallProjection::default()
        };
        assert!(has_more(Some(&page(true, Some("kmp1:1:x")))));
        assert!(!has_more(Some(&page(false, None))));
        assert!(!has_more(Some(&page(true, None))));
        assert!(!has_more(Some(&RecallProjection::default())));
        assert!(!has_more(None));
    }

    #[test]
    fn a_shortened_core_awaits_its_restart() {
        let shortened = RecallProjection {
            core_text_shortened: true,
            ..RecallProjection::default()
        };
        assert!(restarts(Some(&shortened)));
        assert!(!restarts(Some(&RecallProjection::default())));
        assert!(!restarts(None));
    }
}
