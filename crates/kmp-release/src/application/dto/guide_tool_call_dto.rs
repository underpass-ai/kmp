use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct GuideToolCallDto {
    pub name: String,
    pub arguments: Value,
}
