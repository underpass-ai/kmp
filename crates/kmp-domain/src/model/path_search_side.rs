use std::collections::HashMap;

use crate::RelationDirection;

/// One end of a bidirectional path search: the distances it knows, the level
/// it expands next, and every adjacency row it has read.
pub(super) struct PathSearchSide {
    pub(super) direction: RelationDirection,
    pub(super) distance: HashMap<String, u32>,
    pub(super) frontier: Vec<String>,
    pub(super) depth: u32,
    /// Expanded node -> neighbour ids in adjacency-key order, as read.
    pub(super) read: Vec<(String, Vec<String>)>,
}

impl PathSearchSide {
    pub(super) fn new(start: &str, direction: RelationDirection) -> Self {
        Self {
            direction,
            distance: HashMap::from([(start.to_string(), 0)]),
            frontier: vec![start.to_string()],
            depth: 0,
            read: Vec::new(),
        }
    }

    pub(super) fn knows(&self, node: &str) -> bool {
        self.distance.contains_key(node)
    }
}
