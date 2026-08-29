use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerManifestDocumentDto {
    pub body: Value,
}
