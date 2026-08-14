#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentExecution {
    pub selected_node_id: String,
    pub written_path: String,
    pub written_content: String,
    pub listed_files: String,
}
