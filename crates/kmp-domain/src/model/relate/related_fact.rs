use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::{
    DomainError, MemoryDimensionIdentity, TemporalAxis, TemporalCoordinate,
    compare_temporal_instants,
};

use super::shared_label::SharedLabel;

/// What a fact's lifecycle said where the read stood: still standing,
/// replaced by a later entry, or run out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactState {
    Current,
    Superseded,
    Expired,
}

impl FactState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
        }
    }
}

/// An entry as `relate` reads it: where it stands in time, which about
/// placed it, and whether it still stood where the read stood. Text and
/// kind travel beside it on the wire; they play no part in relating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedFact {
    ref_id: String,
    about: String,
    coordinates: Vec<TemporalCoordinate>,
    state: FactState,
}

impl RelatedFact {
    pub fn new(
        ref_id: impl Into<String>,
        about: impl Into<String>,
        coordinates: Vec<TemporalCoordinate>,
        state: FactState,
    ) -> Result<Self, DomainError> {
        let ref_id = ref_id.into();
        let about = about.into();
        if ref_id.trim().is_empty() {
            return Err(DomainError::EmptyValue("related fact ref"));
        }
        if about.trim().is_empty() {
            return Err(DomainError::EmptyValue("related fact about"));
        }
        Ok(Self {
            ref_id,
            about,
            coordinates,
            state,
        })
    }

    pub fn ref_id(&self) -> &str {
        &self.ref_id
    }

    pub fn about(&self) -> &str {
        &self.about
    }

    pub fn coordinates(&self) -> &[TemporalCoordinate] {
        &self.coordinates
    }

    pub fn state(&self) -> FactState {
        self.state
    }

    /// The labels this fact stands in: each coordinate's dimension kind as
    /// the key and its scope, stripped of the about that namespaces it, as
    /// the value. `incident=north-outage` in two abouts is one label; the
    /// same scope under another kind is another label.
    pub fn labels(&self) -> BTreeSet<SharedLabel> {
        self.coordinates
            .iter()
            .map(|coordinate| {
                SharedLabel::new(coordinate.dimension(), bare_scope(coordinate.scope_id()))
            })
            .collect()
    }

    /// The earliest instant this fact stands at inside one bare scope on the
    /// clock read, with the compatible precedence for the default clock and
    /// no substitution for an explicit one.
    pub fn instant_in(&self, label: &SharedLabel, axis: TemporalAxis) -> Option<&str> {
        self.coordinates
            .iter()
            .filter(|coordinate| stands_in(coordinate, label))
            .filter_map(|coordinate| clock_instant(coordinate, axis))
            .min_by(|left, right| compare_temporal_instants(left, right).unwrap_or(Ordering::Equal))
    }

    /// The validity span this fact holds inside one bare scope, when it
    /// carries one there: `(valid_from, valid_until)`, either side open.
    pub fn validity_in(&self, label: &SharedLabel) -> Option<(Option<&str>, Option<&str>)> {
        self.coordinates
            .iter()
            .filter(|coordinate| stands_in(coordinate, label))
            .find(|coordinate| {
                coordinate.valid_from().is_some() || coordinate.valid_until().is_some()
            })
            .map(|coordinate| (coordinate.valid_from(), coordinate.valid_until()))
    }

    pub fn sequence_in(&self, label: &SharedLabel) -> Option<u32> {
        self.coordinates
            .iter()
            .filter(|coordinate| stands_in(coordinate, label))
            .find_map(TemporalCoordinate::sequence)
    }

    pub fn rank_in(&self, label: &SharedLabel) -> Option<u32> {
        self.coordinates
            .iter()
            .filter(|coordinate| stands_in(coordinate, label))
            .find_map(TemporalCoordinate::rank)
    }
}

/// Whether a coordinate places its fact in this label: the same dimension
/// kind and, once the about is stripped, the same scope.
fn stands_in(coordinate: &TemporalCoordinate, label: &SharedLabel) -> bool {
    coordinate.dimension() == label.key() && bare_scope(coordinate.scope_id()) == label.value()
}

/// A scope id with its about namespace stripped, so the same scope declared
/// by two abouts reads as one.
pub(super) fn bare_scope(scope_id: &str) -> String {
    MemoryDimensionIdentity::parse(scope_id)
        .map(|identity| identity.dimension_id().to_string())
        .unwrap_or_else(|| scope_id.trim().to_string())
}

fn clock_instant(coordinate: &TemporalCoordinate, axis: TemporalAxis) -> Option<&str> {
    match axis {
        TemporalAxis::Occurred => coordinate.occurred_at(),
        TemporalAxis::Observed => coordinate.observed_at(),
        TemporalAxis::Ingested => coordinate.ingested_at(),
        TemporalAxis::Validity => coordinate.valid_from(),
        TemporalAxis::Default => coordinate
            .occurred_at()
            .or(coordinate.valid_from())
            .or(coordinate.observed_at())
            .or(coordinate.ingested_at()),
    }
}
