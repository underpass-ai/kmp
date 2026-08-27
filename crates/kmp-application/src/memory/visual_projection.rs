use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{
    DimensionSelection, TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection,
    TemporalWindow,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ApplicationError;

use super::{TemporalIncludeOptions, TemporalMemoryQuery, TemporalMemoryResult};

pub const MAX_VISUAL_SOURCE_ENTRIES: usize = 65_536;
pub const MAX_VISUAL_PAGE_ENTRIES: usize = 2_048;
pub const MAX_VISUAL_BINS: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualLevelOfDetail {
    #[default]
    Atlas,
    Episode,
    Moment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualProjectionQuery {
    pub about: String,
    pub from: String,
    pub to: String,
    pub axis: TemporalAxis,
    pub dimensions: DimensionSelection,
    pub level_of_detail: VisualLevelOfDetail,
    pub bin_count: usize,
    pub page_entries: usize,
    pub cursor: Option<String>,
    pub depth: u32,
}

impl VisualProjectionQuery {
    pub fn temporal_query(&self) -> Result<TemporalMemoryQuery, ApplicationError> {
        Ok(TemporalMemoryQuery {
            about: self.about.clone(),
            direction: TemporalDirection::Goto,
            axis: self.axis,
            cursor: TemporalCursor::time(self.to.clone())?,
            dimensions: self.dimensions.clone(),
            window: TemporalWindow::new(0, 0),
            limit_entries: Some(MAX_VISUAL_SOURCE_ENTRIES),
            include: TemporalIncludeOptions {
                evidence: false,
                relations: true,
                raw_refs: false,
            },
            token_budget: 262_144,
            depth: self.depth.max(1),
            max_tier: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualRange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualBin {
    pub dimension: String,
    pub from: String,
    pub to: String,
    pub total: usize,
    pub by_kind: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualCluster {
    pub dimension: String,
    pub from: String,
    pub to: String,
    pub total: usize,
    pub refs: Vec<String>,
    pub by_kind: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualEntry {
    pub ref_id: String,
    pub kind: String,
    pub text: String,
    pub coordinates: Vec<TemporalCoordinateView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemporalCoordinateView {
    pub dimension: String,
    pub scope_id: String,
    pub occurred_at: Option<String>,
    pub observed_at: Option<String>,
    pub ingested_at: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub sequence: Option<u32>,
    pub rank: Option<u32>,
}

impl From<&TemporalCoordinate> for TemporalCoordinateView {
    fn from(value: &TemporalCoordinate) -> Self {
        Self {
            dimension: value.dimension().to_string(),
            scope_id: value.scope_id().to_string(),
            occurred_at: value.occurred_at().map(ToString::to_string),
            observed_at: value.observed_at().map(ToString::to_string),
            ingested_at: value.ingested_at().map(ToString::to_string),
            valid_from: value.valid_from().map(ToString::to_string),
            valid_until: value.valid_until().map(ToString::to_string),
            sequence: value.sequence(),
            rank: value.rank(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualRelation {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub class: String,
    pub why: Option<String>,
    pub evidence: Option<String>,
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisualMetric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualProjectionPage {
    pub returned: usize,
    pub total: usize,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisualProjectionResult {
    pub contract: String,
    pub about: String,
    pub axis: TemporalAxisView,
    pub level_of_detail: VisualLevelOfDetail,
    pub range: VisualRange,
    pub bins: Vec<VisualBin>,
    pub clusters: Vec<VisualCluster>,
    pub entries: Vec<VisualEntry>,
    /// Distinct entries by kind across the selected range. Unlike bin and
    /// cluster aggregates, these totals never count one entry once per lane.
    pub by_kind: BTreeMap<String, usize>,
    pub relations: Vec<VisualRelation>,
    pub metrics: Vec<VisualMetric>,
    pub included_dimensions: Vec<String>,
    pub missing_dimensions: Vec<String>,
    pub revision: u64,
    pub content_hash: String,
    pub page: VisualProjectionPage,
    pub truncated: bool,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalAxisView {
    Default,
    Occurred,
    Observed,
    Ingested,
    Validity,
}

impl From<TemporalAxis> for TemporalAxisView {
    fn from(value: TemporalAxis) -> Self {
        match value {
            TemporalAxis::Default => Self::Default,
            TemporalAxis::Occurred => Self::Occurred,
            TemporalAxis::Observed => Self::Observed,
            TemporalAxis::Ingested => Self::Ingested,
            TemporalAxis::Validity => Self::Validity,
        }
    }
}

#[derive(Debug, Clone)]
struct PositionedEntry {
    entry: VisualEntry,
    position: i128,
    position_text: String,
    dimensions: BTreeSet<String>,
}

pub fn build_visual_projection(
    query: &VisualProjectionQuery,
    temporal: TemporalMemoryResult,
) -> Result<VisualProjectionResult, ApplicationError> {
    let from = timestamp_nanos(&query.from).ok_or_else(|| {
        ApplicationError::Validation("visual projection `from` is not a timestamp".to_string())
    })?;
    let to = timestamp_nanos(&query.to).ok_or_else(|| {
        ApplicationError::Validation("visual projection `to` is not a timestamp".to_string())
    })?;
    if to <= from {
        return Err(ApplicationError::Validation(
            "visual projection range requires `to` after `from`".to_string(),
        ));
    }
    let bin_count = query.bin_count.clamp(1, MAX_VISUAL_BINS);
    let page_entries = query.page_entries.clamp(1, MAX_VISUAL_PAGE_ENTRIES);
    let revision = temporal.source_bundle.metadata().revision;
    let content_hash = temporal.source_bundle.metadata().content_hash.clone();
    let selection_hash = selection_hash(query, revision, &content_hash);
    let offset = cursor_offset(query.cursor.as_deref(), &selection_hash)?;

    let mut positioned = temporal
        .traversal
        .entries()
        .iter()
        .filter_map(|entry| {
            let (position, position_text) = entry
                .coordinates()
                .iter()
                .filter_map(|coordinate| {
                    let value = axis_time(coordinate, query.axis)?;
                    Some((timestamp_nanos(value)?, value.to_string()))
                })
                .min_by_key(|(position, _)| *position)?;
            if position < from || position >= to {
                return None;
            }
            let dimensions = entry
                .coordinates()
                .iter()
                .map(|coordinate| coordinate.dimension().to_string())
                .collect();
            Some(PositionedEntry {
                entry: VisualEntry {
                    ref_id: entry.ref_id().to_string(),
                    kind: entry.kind().to_string(),
                    text: entry.text().to_string(),
                    coordinates: entry
                        .coordinates()
                        .iter()
                        .map(TemporalCoordinateView::from)
                        .collect(),
                },
                position,
                position_text,
                dimensions,
            })
        })
        .collect::<Vec<_>>();
    positioned.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.entry.ref_id.cmp(&right.entry.ref_id))
    });

    let by_kind = positioned
        .iter()
        .fold(BTreeMap::new(), |mut counts, entry| {
            *counts.entry(entry.entry.kind.clone()).or_default() += 1;
            counts
        });
    let bins = visual_bins(&positioned, from, to, bin_count);
    let clusters = if query.level_of_detail == VisualLevelOfDetail::Episode {
        visual_clusters(&positioned, from, to, bin_count)
    } else {
        Vec::new()
    };
    let total = positioned.len();
    let end = offset.saturating_add(page_entries).min(total);
    let entries = if query.level_of_detail == VisualLevelOfDetail::Moment {
        positioned[offset.min(total)..end]
            .iter()
            .map(|entry| entry.entry.clone())
            .collect()
    } else {
        Vec::new()
    };
    let page_returned = if query.level_of_detail == VisualLevelOfDetail::Moment {
        entries.len()
    } else {
        total
    };
    let has_more = query.level_of_detail == VisualLevelOfDetail::Moment && end < total;
    let next_cursor = has_more.then(|| format!("kmp-visual-v1:{selection_hash}:{end}"));
    let page_refs = entries
        .iter()
        .map(|entry| entry.ref_id.clone())
        .collect::<BTreeSet<_>>();
    let relations = if query.level_of_detail == VisualLevelOfDetail::Moment {
        visual_relations(&temporal, &page_refs)
    } else {
        Vec::new()
    };
    let included_dimensions = positioned
        .iter()
        .flat_map(|entry| entry.dimensions.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let causal = causal_relation_count(&relations);
    let relation_count = relations.len();
    let source_truncated = temporal.traversal.page().has_more();
    let missing = if source_truncated {
        vec!["visual_source_entries".to_string()]
    } else {
        Vec::new()
    };

    Ok(VisualProjectionResult {
        contract: "kmp.visual.projection.v1".to_string(),
        about: query.about.clone(),
        axis: query.axis.into(),
        level_of_detail: query.level_of_detail,
        range: VisualRange {
            from: query.from.clone(),
            to: query.to.clone(),
        },
        bins,
        clusters,
        entries,
        by_kind,
        relations,
        metrics: vec![
            VisualMetric {
                name: "entry_count".to_string(),
                value: total as f64,
                unit: "entries".to_string(),
                scope: "selected_range".to_string(),
            },
            VisualMetric {
                name: "relation_count".to_string(),
                value: relation_count as f64,
                unit: "relations".to_string(),
                scope: "selected_subgraph".to_string(),
            },
            VisualMetric {
                name: "causal_density".to_string(),
                value: ratio(causal, relation_count),
                unit: "ratio".to_string(),
                scope: "selected_subgraph".to_string(),
            },
        ],
        included_dimensions,
        missing_dimensions: temporal.traversal.missing_dimensions().to_vec(),
        revision,
        content_hash,
        page: VisualProjectionPage {
            returned: page_returned,
            total,
            has_more,
            next_cursor,
        },
        truncated: source_truncated || has_more,
        missing,
    })
}

fn visual_bins(entries: &[PositionedEntry], from: i128, to: i128, count: usize) -> Vec<VisualBin> {
    let span = (to - from).max(1);
    let mut bins = BTreeMap::<(String, usize), (usize, BTreeMap<String, usize>)>::new();
    for entry in entries {
        let index = (((entry.position - from) * count as i128) / span)
            .clamp(0, count.saturating_sub(1) as i128) as usize;
        for dimension in &entry.dimensions {
            let (total, by_kind) = bins.entry((dimension.clone(), index)).or_default();
            *total += 1;
            *by_kind.entry(entry.entry.kind.clone()).or_default() += 1;
        }
    }
    bins.into_iter()
        .map(|((dimension, index), (total, by_kind))| VisualBin {
            dimension,
            from: nanos_timestamp(from + span * index as i128 / count as i128),
            to: nanos_timestamp(from + span * (index + 1) as i128 / count as i128),
            total,
            by_kind,
        })
        .collect()
}

fn visual_clusters(
    entries: &[PositionedEntry],
    from: i128,
    to: i128,
    count: usize,
) -> Vec<VisualCluster> {
    let span = (to - from).max(1);
    let mut clusters = BTreeMap::<(String, usize), Vec<&PositionedEntry>>::new();
    for entry in entries {
        let index = (((entry.position - from) * count as i128) / span)
            .clamp(0, count.saturating_sub(1) as i128) as usize;
        for dimension in &entry.dimensions {
            clusters
                .entry((dimension.clone(), index))
                .or_default()
                .push(entry);
        }
    }
    clusters
        .into_iter()
        .map(|((dimension, _), entries)| {
            let mut by_kind = BTreeMap::new();
            let mut refs = Vec::new();
            for entry in &entries {
                *by_kind.entry(entry.entry.kind.clone()).or_default() += 1;
                refs.push(entry.entry.ref_id.clone());
            }
            VisualCluster {
                dimension,
                from: entries
                    .first()
                    .map(|entry| entry.position_text.clone())
                    .unwrap_or_default(),
                to: entries
                    .last()
                    .map(|entry| entry.position_text.clone())
                    .unwrap_or_default(),
                total: entries.len(),
                refs,
                by_kind,
            }
        })
        .collect()
}

fn visual_relations(
    temporal: &TemporalMemoryResult,
    refs: &BTreeSet<String>,
) -> Vec<VisualRelation> {
    temporal
        .source_bundle
        .relationships()
        .iter()
        .filter(|relation| {
            relation.explanation().semantic_class()
                != &kmp_domain::RelationSemanticClass::Structural
                && ((refs.contains(relation.source_node_id())
                    && refs.contains(relation.target_node_id()))
                    || (relation.relationship_type() == "supports"
                        && refs.contains(relation.target_node_id())))
        })
        .map(|relation| VisualRelation {
            from: relation.source_node_id().to_string(),
            to: relation.target_node_id().to_string(),
            rel: relation.relationship_type().to_string(),
            class: relation.explanation().semantic_class().as_str().to_string(),
            why: relation.explanation().rationale().map(ToString::to_string),
            evidence: relation.explanation().evidence().map(ToString::to_string),
            confidence: relation.explanation().confidence().map(ToString::to_string),
        })
        .collect()
}

fn axis_time(coordinate: &TemporalCoordinate, axis: TemporalAxis) -> Option<&str> {
    match axis {
        TemporalAxis::Default => coordinate
            .occurred_at()
            .or(coordinate.valid_from())
            .or(coordinate.observed_at())
            .or(coordinate.ingested_at()),
        TemporalAxis::Occurred => coordinate.occurred_at(),
        TemporalAxis::Observed => coordinate.observed_at(),
        TemporalAxis::Ingested => coordinate.ingested_at(),
        TemporalAxis::Validity => coordinate.valid_from().or(coordinate.valid_until()),
    }
}

fn selection_hash(query: &VisualProjectionQuery, revision: u64, content_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.about.as_bytes());
    hasher.update(query.from.as_bytes());
    hasher.update(query.to.as_bytes());
    hasher.update(format!("{:?}", query.axis).as_bytes());
    hasher.update(format!("{:?}", query.dimensions).as_bytes());
    hasher.update(format!("{:?}", query.level_of_detail).as_bytes());
    hasher.update(query.bin_count.to_le_bytes());
    hasher.update(revision.to_le_bytes());
    hasher.update(content_hash.as_bytes());
    format!("{:x}", hasher.finalize())[..24].to_string()
}

fn cursor_offset(cursor: Option<&str>, selection_hash: &str) -> Result<usize, ApplicationError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let valid = parts.next() == Some("kmp-visual-v1")
        && parts.next() == Some(selection_hash)
        && parts.clone().count() == 1;
    let offset = parts.next().and_then(|value| value.parse::<usize>().ok());
    if !valid || offset.is_none() {
        return Err(ApplicationError::Validation(
            "visual projection cursor is malformed or belongs to another selection".to_string(),
        ));
    }
    Ok(offset.unwrap_or_default())
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn causal_relation_count(relations: &[VisualRelation]) -> usize {
    relations
        .iter()
        .filter(|relation| relation.class == "causal")
        .count()
}

fn timestamp_nanos(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("unix:") {
        let (seconds, nanos) = value.split_once(':')?;
        let seconds = seconds.parse::<i128>().ok()? - 100_000_000_000i128;
        let nanos = nanos.parse::<i128>().ok()?;
        return Some(seconds * 1_000_000_000 + nanos);
    }
    basic_rfc3339_nanos(value)
}

fn nanos_timestamp(value: i128) -> String {
    let seconds = value.div_euclid(1_000_000_000);
    let nanos = value.rem_euclid(1_000_000_000);
    format!("unix:{:012}:{:09}", seconds + 100_000_000_000i128, nanos)
}

fn basic_rfc3339_nanos(value: &str) -> Option<i128> {
    let value = value.trim();
    if value.len() < 20 {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> { value.get(from..to)?.parse().ok() };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    if value.get(4..5)? != "-"
        || value.get(7..8)? != "-"
        || value.get(10..11)? != "T"
        || value.get(13..14)? != ":"
        || value.get(16..17)? != ":"
    {
        return None;
    }
    let tail = value.get(19..)?;
    let timezone_start = tail
        .char_indices()
        .find_map(|(index, character)| matches!(character, 'Z' | '+' | '-').then_some(index))?;
    let fraction = tail.get(..timezone_start)?;
    let timezone = tail.get(timezone_start..)?;
    let nanos = match fraction.strip_prefix('.') {
        Some(digits) if !digits.is_empty() && digits.len() <= 9 => {
            let padded = format!("{digits:0<9}");
            padded.parse::<i128>().ok()?
        }
        None if fraction.is_empty() => 0,
        _ => return None,
    };
    let offset_seconds = match timezone {
        "Z" => 0,
        offset if offset.len() == 6 && offset.get(3..4) == Some(":") => {
            let sign = match offset.get(..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let hours = offset.get(1..3)?.parse::<i64>().ok()?;
            let minutes = offset.get(4..6)?.parse::<i64>().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3_600 + minutes * 60)
        }
        _ => return None,
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second
        - offset_seconds;
    Some(seconds as i128 * 1_000_000_000 + nanos)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sortable_and_rfc3339_timestamps_share_one_coordinate_space() {
        assert_eq!(
            timestamp_nanos("2026-08-27T12:00:00Z"),
            timestamp_nanos("unix:101787832000:000000000")
        );
        assert_eq!(
            timestamp_nanos("2026-08-27T14:00:00.125+02:00"),
            timestamp_nanos("unix:101787832000:125000000")
        );
    }

    #[test]
    fn projection_cursor_is_bound_to_its_selection() {
        assert_eq!(cursor_offset(None, "abc").expect("first page"), 0);
        assert_eq!(
            cursor_offset(Some("kmp-visual-v1:abc:12"), "abc").expect("same selection"),
            12
        );
        assert!(cursor_offset(Some("kmp-visual-v1:def:12"), "abc").is_err());
    }

    #[test]
    fn visual_causal_density_does_not_count_other_explanatory_classes() {
        let relation = |class: &str| VisualRelation {
            from: "a".to_string(),
            to: "b".to_string(),
            rel: "rel".to_string(),
            class: class.to_string(),
            why: None,
            evidence: None,
            confidence: None,
        };
        let relations = [
            relation("causal"),
            relation("evidential"),
            relation("motivational"),
        ];

        assert_eq!(causal_relation_count(&relations), 1);
    }
}
