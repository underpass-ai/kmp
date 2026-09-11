use std::sync::{Arc, Mutex};

use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, NodeDetailProjection, NodeDetailReader,
    NodeNeighborhood, NodeProjection, PortError,
};

use super::QueryApplicationService;

#[derive(Default)]
struct Records {
    nodes: Vec<Option<NodeProjection>>,
    bodies: Vec<Option<NodeDetailProjection>>,
    requested_bodies: Mutex<Vec<String>>,
    body_error: bool,
}

impl GraphNeighborhoodReader for Records {
    async fn load_nodes_batch(
        &self,
        _: Vec<String>,
    ) -> Result<Vec<Option<NodeProjection>>, PortError> {
        Ok(self.nodes.clone())
    }
    async fn load_neighborhood(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        panic!("selected batch must not read source neighborhoods")
    }
    async fn load_context_path(
        &self,
        _: &str,
        _: &str,
        _: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        panic!("selected batch must not discover a path")
    }
}
impl NodeDetailReader for Records {
    async fn load_node_detail(&self, _: &str) -> Result<Option<NodeDetailProjection>, PortError> {
        panic!("selected batch must reuse the body batch")
    }
    async fn load_node_details_batch(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        *self.requested_bodies.lock().expect("fixture lock") = ids;
        if self.body_error {
            return Err(PortError::Unavailable("body read failed".into()));
        }
        Ok(self.bodies.clone())
    }
}

fn node(id: &str) -> NodeProjection {
    NodeProjection {
        node_id: id.into(),
        node_kind: "evidence".into(),
        title: "Signed source".into(),
        summary: "Stored summary".into(),
        status: "ACTIVE".into(),
        labels: vec!["source".into()],
        properties: [("source".into(), "signed:document".into())].into(),
        provenance: None,
    }
}
fn body(id: &str) -> NodeDetailProjection {
    NodeDetailProjection {
        node_id: id.into(),
        detail: "Canonical source — 17 != 18".into(),
        content_hash: "opaque-stored-hash".into(),
        revision: 7,
    }
}
fn query(records: &Arc<Records>) -> QueryApplicationService<Records, Records, ()> {
    QueryApplicationService::new(records.clone(), records.clone(), Arc::new(()), "batch-test")
}

#[tokio::test]
async fn selected_nodes_keep_order_duplicates_properties_and_missing_body_states() {
    let records = Arc::new(Records {
        nodes: vec![Some(node("a")), None, Some(node("b")), Some(node("a"))],
        bodies: vec![Some(body("a")), None, Some(body("a"))],
        ..Default::default()
    });
    let result = query(&records)
        .get_node_details(vec![" a ".into(), "orphan".into(), "b".into(), "a".into()])
        .await
        .expect("typed batch");
    assert_eq!(
        *records.requested_bodies.lock().expect("fixture lock"),
        ["a", "b", "a"]
    );
    assert!(
        result[1].is_none(),
        "an absent graph node is not an orphaned body"
    );
    assert!(result[2].as_ref().expect("present node").detail.is_none());
    assert!(
        result[0] == result[3],
        "duplicate input retains its complete slot"
    );
    let a = result[0].as_ref().expect("present a");
    assert_eq!(a.node.properties["source"], "signed:document");
    assert_eq!(a.node.labels, ["source"]);
    assert_eq!(a.node.summary, "Stored summary");
    let a_body = a.detail.as_ref().expect("source body");
    assert_eq!(a_body.detail, "Canonical source — 17 != 18");
    assert_eq!(a_body.content_hash, "opaque-stored-hash");
    assert_eq!(a_body.revision, 7);
}

#[tokio::test]
async fn malformed_batch_slots_are_refused_instead_of_attaching_the_wrong_body() {
    for records in [
        Records {
            nodes: vec![],
            ..Default::default()
        },
        Records {
            nodes: vec![Some(node("other"))],
            ..Default::default()
        },
        Records {
            nodes: vec![Some(node("a"))],
            bodies: vec![],
            ..Default::default()
        },
        Records {
            nodes: vec![Some(node("a"))],
            bodies: vec![Some(body("other"))],
            ..Default::default()
        },
    ] {
        let result = query(&Arc::new(records))
            .get_node_details(vec!["a".into()])
            .await;
        assert!(matches!(
            result,
            Err(crate::ApplicationError::Ports(PortError::InvalidState(_)))
        ));
    }
}

#[tokio::test]
async fn empty_and_absent_selections_skip_bodies_but_real_read_errors_propagate() {
    let records = Arc::new(Records {
        nodes: vec![None],
        body_error: true,
        ..Default::default()
    });
    assert!(
        query(&records)
            .get_node_details(vec![])
            .await
            .expect("empty")
            .is_empty()
    );
    assert!(
        query(&records)
            .get_node_details(vec!["orphan".into()])
            .await
            .expect("missing")[0]
            .is_none()
    );
    assert!(
        records
            .requested_bodies
            .lock()
            .expect("fixture lock")
            .is_empty()
    );
    assert!(matches!(
        query(&records).get_node_details(vec![" ".into()]).await,
        Err(crate::ApplicationError::Validation(_))
    ));
    let broken = Arc::new(Records {
        nodes: vec![Some(node("a"))],
        body_error: true,
        ..Default::default()
    });
    assert!(matches!(
        query(&broken).get_node_details(vec!["a".into()]).await,
        Err(crate::ApplicationError::Ports(PortError::Unavailable(_)))
    ));
}
