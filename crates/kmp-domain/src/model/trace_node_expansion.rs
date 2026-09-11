use super::trace_read_budget::TraceReadBudget;
use crate::{
    NodeRelationProjection, PortError, RelationDirection, RelationPosition, TraceSnapshotReader,
};

/// One node's indexed adjacency stream; admitted rows are shared by path states.
#[derive(Default)]
pub(super) struct TraceNodeExpansion {
    move_index: usize,
    after: Option<RelationPosition>,
    pub complete: bool,
    pub has_eligible: bool,
    pub eligible: Vec<NodeRelationProjection>,
}

impl TraceNodeExpansion {
    pub fn page<R: TraceSnapshotReader>(
        &mut self,
        node: &str,
        moves: &[(RelationDirection, Option<&str>)],
        budget: &mut TraceReadBudget<'_, R>,
        rows: u32,
    ) -> Result<Option<Vec<NodeRelationProjection>>, PortError> {
        let (direction, relation_type) = moves[self.move_index];
        let Some(page) =
            budget.page_limited(node, direction, self.after.take(), relation_type, rows)?
        else {
            return Ok(None);
        };
        if page.exhausted {
            self.move_index += 1;
            self.complete = self.move_index == moves.len();
        } else {
            self.after = page.next;
        }
        Ok(Some(page.edges))
    }
}
