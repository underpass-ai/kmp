use super::trace_read_budget::TraceReadBudget;
use crate::{
    MemoryDimensionIdentity, PortError, RelationDirection, TemporalCoordinate, TemporalCursor,
    TemporalReadWindow, TraceSearchRequest, TraceSnapshotReader, compare_temporal_instants,
    temporal_clock_instant, temporal_instant_nanos,
};
use std::collections::BTreeMap;

pub(super) struct TraceTemporalAdmission<'a, R> {
    request: &'a TraceSearchRequest,
    pub budget: TraceReadBudget<'a, R>,
    pub resolved_as_of: Option<String>,
    owned: BTreeMap<String, bool>,
    coordinates: BTreeMap<String, Vec<TemporalCoordinate>>,
    admitted: BTreeMap<String, bool>,
}

impl<'a, R: TraceSnapshotReader> TraceTemporalAdmission<'a, R> {
    pub fn new(reader: &'a R, request: &'a TraceSearchRequest) -> Self {
        Self {
            request,
            budget: TraceReadBudget::new(reader, request.limits),
            resolved_as_of: None,
            owned: BTreeMap::new(),
            coordinates: BTreeMap::new(),
            admitted: BTreeMap::new(),
        }
    }

    pub fn is_owned(&mut self, id: &str) -> Result<bool, PortError> {
        if let Some(owned) = self.owned.get(id) {
            return Ok(*owned);
        }
        let owned = self.budget.node(id)?.is_some_and(|node| {
            node.properties.get("memory_about") == Some(&self.request.about)
                && node.labels.iter().any(|label| label == "entry")
        });
        if self.budget.stop.is_none() {
            self.owned.insert(id.into(), owned);
        }
        Ok(owned)
    }

    pub fn resolve_cut(&mut self) -> Result<bool, PortError> {
        match self.request.temporal.cursor() {
            Some(TemporalCursor::Time(time)) => self.resolved_as_of = Some(time.clone()),
            Some(TemporalCursor::Ref(reference)) => {
                if !self.is_owned(reference)? {
                    if self.budget.stop.is_some() {
                        return Ok(false);
                    }
                    return Err(PortError::InvalidState(
                        "as_of.ref must be an existing entry owned by about".into(),
                    ));
                }
                if !self.load_coordinates(reference)? {
                    return Ok(false);
                }
                let axis = self.request.temporal.axis().unwrap_or_default();
                self.resolved_as_of = self.coordinates[reference]
                    .iter()
                    .filter_map(|c| temporal_clock_instant(c, axis))
                    .filter(|(at, _)| temporal_instant_nanos(at).is_some())
                    .min_by(|a, b| {
                        compare_temporal_instants(a.0, b.0).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(at, _)| at.to_string());
                if self.resolved_as_of.is_none() {
                    return Err(PortError::InvalidState(
                        "as_of.ref has no instant on the selected clock".into(),
                    ));
                }
            }
            _ => {}
        }
        Ok(true)
    }

    pub fn admits(&mut self, id: &str) -> Result<bool, PortError> {
        if let Some(admitted) = self.admitted.get(id) {
            return Ok(*admitted);
        }
        if !self.is_owned(id)? {
            return Ok(false);
        }
        if self.request.temporal.is_frontier() {
            return Ok(true);
        }
        if !self.load_coordinates(id)? {
            return Ok(false);
        }
        let admitted = self.coordinates[id]
            .iter()
            .any(|c| self.window().admits_coordinate(c));
        self.admitted.insert(id.into(), admitted);
        Ok(admitted)
    }

    pub fn window(&self) -> TemporalReadWindow<'_> {
        TemporalReadWindow::new(&self.request.temporal, self.resolved_as_of.as_deref())
    }

    fn load_coordinates(&mut self, id: &str) -> Result<bool, PortError> {
        if self.coordinates.contains_key(id) {
            return Ok(true);
        }
        let mut after = None;
        let mut coordinates = Vec::new();
        loop {
            let Some(page) = self.budget.page(
                id,
                RelationDirection::Incoming,
                after,
                Some("contains_entry"),
            )?
            else {
                return Ok(false);
            };
            for edge in page.edges {
                if MemoryDimensionIdentity::resolve(&self.request.about, &edge.source_node_id)
                    .is_none()
                {
                    continue;
                }
                if let Some(coordinate) =
                    TemporalCoordinate::from_relation_explanation(&edge.explanation)
                        .map_err(|e| PortError::InvalidState(e.to_string()))?
                    && coordinate.scope_id() == edge.source_node_id
                {
                    coordinates.push(coordinate);
                }
            }
            if page.exhausted {
                break;
            }
            after = page.next;
        }
        self.coordinates.insert(id.into(), coordinates);
        Ok(true)
    }
}
