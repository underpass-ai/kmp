use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideCapabilitiesDto {
    pub mcp_tools: Vec<String>,
}
