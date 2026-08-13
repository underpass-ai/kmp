pub const SUMMARY_PATH: &str = "context-summary.md";

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRequest {
    pub root_node_id: String,
    pub root_node_kind: String,
    pub role: String,
    pub focus_node_kind: String,
    pub requested_scopes: Vec<String>,
    pub token_budget: u32,
    pub summary_path: String,
}

impl AgentRequest {
    pub fn reference_defaults(root_node_id: &str, root_node_kind: &str) -> Self {
        Self {
            root_node_id: root_node_id.to_string(),
            root_node_kind: root_node_kind.to_string(),
            role: "implementer".to_string(),
            focus_node_kind: "work_item".to_string(),
            requested_scopes: vec!["implementation".to_string(), "dependencies".to_string()],
            token_budget: 1200,
            summary_path: SUMMARY_PATH.to_string(),
        }
    }
}
