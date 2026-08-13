#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmpStats {
    selected_nodes: u32,
    selected_relationships: u32,
    detailed_nodes: u32,
}

impl KmpStats {
    pub fn new(selected_nodes: u32, selected_relationships: u32, detailed_nodes: u32) -> Self {
        Self {
            selected_nodes,
            selected_relationships,
            detailed_nodes,
        }
    }

    pub fn selected_nodes(&self) -> u32 {
        self.selected_nodes
    }

    pub fn selected_relationships(&self) -> u32 {
        self.selected_relationships
    }

    pub fn detailed_nodes(&self) -> u32 {
        self.detailed_nodes
    }
}
