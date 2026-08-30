/// One dry-run visible change inside an ingest plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KmpIngestChange {
    pub(crate) entity_kind: String,
    pub(crate) entity_id: String,
    pub(crate) payload_json: String,
    pub(crate) reason: String,
    pub(crate) scopes: Vec<String>,
}
