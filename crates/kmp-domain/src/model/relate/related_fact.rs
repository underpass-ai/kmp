use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::{
    DomainError, MemoryDimensionIdentity, TemporalAxis, TemporalCoordinate,
    compare_temporal_instants,
};

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

    /// The dimension scopes this fact stands in, stripped of the about that
    /// namespaces them: `incident:north-outage` in two abouts is one scope.
    pub fn bare_scopes(&self) -> BTreeSet<String> {
        self.coordinates
            .iter()
            .map(|coordinate| bare_scope(coordinate.scope_id()))
            .collect()
    }

    /// The earliest instant this fact stands at inside one bare scope on the
    /// clock read, with the compatible precedence for the default clock and
    /// no substitution for an explicit one.
    pub fn instant_in(&self, scope: &str, axis: TemporalAxis) -> Option<&str> {
        self.coordinates
            .iter()
            .filter(|coordinate| bare_scope(coordinate.scope_id()) == scope)
            .filter_map(|coordinate| clock_instant(coordinate, axis))
            .min_by(|left, right| compare_temporal_instants(left, right).unwrap_or(Ordering::Equal))
    }

    /// The validity span this fact holds inside one bare scope, when it
    /// carries one there: `(valid_from, valid_until)`, either side open.
    pub fn validity_in(&self, scope: &str) -> Option<(Option<&str>, Option<&str>)> {
        self.coordinates
            .iter()
            .filter(|coordinate| bare_scope(coordinate.scope_id()) == scope)
            .find(|coordinate| {
                coordinate.valid_from().is_some() || coordinate.valid_until().is_some()
            })
            .map(|coordinate| (coordinate.valid_from(), coordinate.valid_until()))
    }

    pub fn sequence_in(&self, scope: &str) -> Option<u32> {
        self.coordinates
            .iter()
            .filter(|coordinate| bare_scope(coordinate.scope_id()) == scope)
            .find_map(TemporalCoordinate::sequence)
    }

    pub fn rank_in(&self, scope: &str) -> Option<u32> {
        self.coordinates
            .iter()
            .filter(|coordinate| bare_scope(coordinate.scope_id()) == scope)
            .find_map(TemporalCoordinate::rank)
    }
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
