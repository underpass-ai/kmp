use std::collections::BTreeMap;
use std::fmt::Write;

use crate::model::{BundleNode, BundleNodeDetail, BundleRelationship, KmpBundle};

pub(super) fn raw_dump_records<'a>(
    bundle: &'a KmpBundle,
    detail_by_node_id: &'a BTreeMap<&'a str, &'a BundleNodeDetail>,
) -> impl Iterator<Item = String> + 'a {
    std::iter::once(node_record(bundle.root_node(), detail_by_node_id))
        .chain(
            bundle
                .neighbor_nodes()
                .iter()
                .map(|node| node_record(node, detail_by_node_id)),
        )
        .chain(bundle.relationships().iter().map(relationship_record))
}

fn node_record(node: &BundleNode, detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>) -> String {
    let mut record = String::new();
    write!(
        record,
        "Node: {}. Kind: {}. Summary: {}.",
        node.node_id(),
        node.node_kind(),
        node.summary()
    )
    .expect("writing to String cannot fail");
    if let Some(detail) = detail_by_node_id.get(node.node_id()) {
        write!(record, " Detail: {}.", detail.detail()).expect("writing to String cannot fail");
    }
    record.push('\n');
    record
}

fn relationship_record(rel: &BundleRelationship) -> String {
    let mut record = String::new();
    write!(
        record,
        "Relationship: {} connects to {} via {}. Semantic class: {}.",
        rel.source_node_id(),
        rel.target_node_id(),
        rel.relationship_type(),
        rel.explanation().semantic_class().as_str(),
    )
    .expect("writing to String cannot fail");
    if let Some(r) = rel.explanation().rationale() {
        write!(record, " Rationale: {r}.").expect("writing to String cannot fail");
    }
    if let Some(m) = rel.explanation().motivation() {
        write!(record, " Motivation: {m}.").expect("writing to String cannot fail");
    }
    if let Some(m) = rel.explanation().method() {
        write!(record, " Method: {m}.").expect("writing to String cannot fail");
    }
    if let Some(d) = rel.explanation().decision_id() {
        write!(record, " Decision: {d}.").expect("writing to String cannot fail");
    }
    if let Some(c) = rel.explanation().caused_by_node_id() {
        write!(record, " Caused by: {c}.").expect("writing to String cannot fail");
    }
    record.push('\n');
    record
}
