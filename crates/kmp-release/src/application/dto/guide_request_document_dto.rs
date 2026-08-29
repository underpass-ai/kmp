use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct GuideRequestDocumentDto {
    pub body: Value,
}

impl GuideRequestDocumentDto {
    pub fn about(&self) -> &str {
        self.body["about"].as_str().unwrap_or_default()
    }
}
