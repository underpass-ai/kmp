use crate::{
    DimensionSelection, DomainError, TemporalAxis, TemporalCursor, TemporalDirection,
    TemporalInterval, TemporalWindow,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalTraversalRequest {
    direction: TemporalDirection,
    axis: TemporalAxis,
    cursor: Option<TemporalCursor>,
    interval: Option<TemporalInterval>,
    dimensions: DimensionSelection,
    requested_dimensions: Option<DimensionSelection>,
    window: TemporalWindow,
    limit_entries: Option<usize>,
}

impl TemporalTraversalRequest {
    pub fn new(direction: TemporalDirection, cursor: impl Into<Option<TemporalCursor>>) -> Self {
        Self {
            direction,
            axis: TemporalAxis::Default,
            cursor: cursor.into(),
            interval: None,
            dimensions: DimensionSelection::all(),
            requested_dimensions: None,
            window: TemporalWindow::default(),
            limit_entries: None,
        }
    }

    pub fn with_dimensions(mut self, dimensions: DimensionSelection) -> Self {
        self.dimensions = dimensions;
        self
    }

    pub fn with_axis(mut self, axis: TemporalAxis) -> Self {
        self.axis = axis;
        self
    }

    pub fn with_requested_dimensions(mut self, dimensions: DimensionSelection) -> Self {
        self.requested_dimensions = Some(dimensions);
        self
    }

    pub fn with_window(mut self, window: TemporalWindow) -> Self {
        self.window = window;
        self
    }

    pub fn with_limit_entries(mut self, limit_entries: usize) -> Result<Self, DomainError> {
        if limit_entries == 0 {
            return Err(DomainError::InvalidState(
                "temporal limit_entries must be greater than zero".to_string(),
            ));
        }
        self.limit_entries = Some(limit_entries);
        Ok(self)
    }

    pub fn direction(&self) -> TemporalDirection {
        self.direction
    }

    pub fn axis(&self) -> TemporalAxis {
        self.axis
    }

    pub fn cursor(&self) -> Option<&TemporalCursor> {
        self.cursor.as_ref()
    }

    pub fn with_interval(mut self, interval: TemporalInterval) -> Self {
        self.interval = Some(interval);
        self
    }

    pub fn interval(&self) -> Option<&TemporalInterval> {
        self.interval.as_ref()
    }

    pub(super) fn validate(&self) -> Result<(), DomainError> {
        if self.cursor.is_none()
            && (self.interval.is_none()
                || !matches!(
                    self.direction,
                    TemporalDirection::Forward | TemporalDirection::Rewind
                ))
        {
            return Err(DomainError::InvalidState(
                "temporal cursor is required except for Forward/Rewind with an interval"
                    .to_string(),
            ));
        }
        if self.interval.is_some() && matches!(self.cursor, Some(TemporalCursor::Sequence(_))) {
            return Err(DomainError::InvalidState(
                "an interval uses a clock; its continuation must be a ref or time, not a sequence"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn dimensions(&self) -> &DimensionSelection {
        &self.dimensions
    }

    pub fn requested_dimensions(&self) -> &DimensionSelection {
        self.requested_dimensions
            .as_ref()
            .unwrap_or(&self.dimensions)
    }

    pub(super) fn window(&self) -> TemporalWindow {
        self.window
    }

    pub(super) fn limit_entries(&self) -> Option<usize> {
        self.limit_entries
    }
}
