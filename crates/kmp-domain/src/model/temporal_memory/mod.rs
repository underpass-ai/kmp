mod axis_key;
mod extract;
mod position;
mod select;

pub use axis_key::compare_temporal_instants;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    DimensionSelection, DimensionSelectionMode, DomainError, KmpBundle, TemporalAxis,
    TemporalCoordinate, TemporalCursor, TemporalDirection, TemporalWindow,
};

use self::extract::{bundle_nodes_by_id, temporal_positions};
use self::position::TemporalPosition;
use self::select::{coordinates_by_ref, ordered_unique_ref_ids, resolve_cursor, select_positions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalTraversalRequest {
    direction: TemporalDirection,
    axis: TemporalAxis,
    cursor: TemporalCursor,
    dimensions: DimensionSelection,
    requested_dimensions: Option<DimensionSelection>,
    window: TemporalWindow,
    limit_entries: Option<usize>,
}

impl TemporalTraversalRequest {
    pub fn new(direction: TemporalDirection, cursor: TemporalCursor) -> Self {
        Self {
            direction,
            axis: TemporalAxis::Default,
            cursor,
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

    pub fn cursor(&self) -> &TemporalCursor {
        &self.cursor
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalEntry {
    ref_id: String,
    kind: String,
    text: String,
    coordinates: Vec<TemporalCoordinate>,
}

impl TemporalEntry {
    pub fn ref_id(&self) -> &str {
        &self.ref_id
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn coordinates(&self) -> &[TemporalCoordinate] {
        &self.coordinates
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalTraversalResult {
    direction: TemporalDirection,
    axis: TemporalAxis,
    resolved_cursor: TemporalCoordinate,
    requested_dimensions: DimensionSelection,
    included_dimensions: Vec<String>,
    missing_dimensions: Vec<String>,
    entries: Vec<TemporalEntry>,
    page: TemporalTraversalPage,
    missing: Vec<String>,
}

impl TemporalTraversalResult {
    pub fn direction(&self) -> TemporalDirection {
        self.direction
    }

    pub fn axis(&self) -> TemporalAxis {
        self.axis
    }

    pub fn resolved_cursor(&self) -> &TemporalCoordinate {
        &self.resolved_cursor
    }

    pub fn requested_dimensions(&self) -> &DimensionSelection {
        &self.requested_dimensions
    }

    pub fn included_dimensions(&self) -> &[String] {
        &self.included_dimensions
    }

    pub fn missing_dimensions(&self) -> &[String] {
        &self.missing_dimensions
    }

    pub fn entries(&self) -> &[TemporalEntry] {
        &self.entries
    }

    pub fn page(&self) -> &TemporalTraversalPage {
        &self.page
    }

    pub fn missing(&self) -> &[String] {
        &self.missing
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalTraversalPage {
    returned: usize,
    total: usize,
    next_cursor: Option<String>,
}

impl TemporalTraversalPage {
    pub fn new(returned: usize, total: usize, next_cursor: Option<String>) -> Self {
        Self {
            returned,
            total,
            next_cursor,
        }
    }

    pub fn returned(&self) -> usize {
        self.returned
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn has_more(&self) -> bool {
        self.returned < self.total
    }

    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

pub struct TemporalMemoryTraversal;

impl TemporalMemoryTraversal {
    pub fn traverse(
        bundle: &KmpBundle,
        request: &TemporalTraversalRequest,
    ) -> Result<TemporalTraversalResult, DomainError> {
        let nodes = bundle_nodes_by_id(bundle);
        let mut positions = temporal_positions(bundle, &nodes, request.axis())?
            .into_iter()
            .filter(|position| {
                request.dimensions.includes_coordinate(
                    position.coordinate.dimension(),
                    position.coordinate.scope_id(),
                )
            })
            .collect::<Vec<_>>();
        positions.sort();
        let available_dimensions = dimensions_from_positions(&positions);

        let cursor = resolve_cursor(&positions, request.cursor(), request.axis())?;
        let has_comparable_positions = cursor.axis_key.as_ref().is_some_and(|axis_key| {
            positions
                .iter()
                .any(|position| position.axis_key.axis() == axis_key.axis())
        });
        let selection = select_positions(&positions, &cursor, request);
        let selected_ref_ids = ordered_unique_ref_ids(selection.positions);
        let page = TemporalTraversalPage::new(
            selected_ref_ids.len(),
            selection.total_unique_refs,
            selection.next_cursor,
        );
        let coordinates_by_ref = coordinates_by_ref(&positions);
        let entries = build_entries(
            selected_ref_ids,
            &nodes,
            &coordinates_by_ref,
            request.dimensions(),
        );
        let included_dimensions = included_dimensions(&entries);
        let coverage_dimensions = if entries.is_empty() && has_comparable_positions {
            &available_dimensions
        } else {
            &included_dimensions
        };
        let missing_dimensions = missing_dimensions(request.dimensions(), coverage_dimensions);
        let missing = if positions.is_empty() || !has_comparable_positions {
            vec!["temporal_positions".to_string()]
        } else {
            Vec::new()
        };

        Ok(TemporalTraversalResult {
            direction: request.direction,
            axis: request.axis,
            resolved_cursor: cursor.coordinate,
            requested_dimensions: request.requested_dimensions().clone(),
            included_dimensions,
            missing_dimensions,
            entries,
            page,
            missing,
        })
    }
}

fn build_entries(
    selected_ref_ids: Vec<String>,
    nodes: &BTreeMap<String, (String, String)>,
    coordinates_by_ref: &BTreeMap<String, Vec<TemporalCoordinate>>,
    dimensions: &DimensionSelection,
) -> Vec<TemporalEntry> {
    selected_ref_ids
        .into_iter()
        .filter_map(|ref_id| {
            let (kind, text) = nodes.get(&ref_id)?;
            let coordinates = coordinates_by_ref
                .get(&ref_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|coordinate| {
                    dimensions.includes_coordinate(coordinate.dimension(), coordinate.scope_id())
                })
                .collect::<Vec<_>>();

            Some(TemporalEntry {
                ref_id,
                kind: kind.clone(),
                text: text.clone(),
                coordinates,
            })
        })
        .collect()
}

fn included_dimensions(entries: &[TemporalEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| entry.coordinates.iter())
        .map(|coordinate| coordinate.dimension().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn dimensions_from_positions(positions: &[TemporalPosition]) -> Vec<String> {
    positions
        .iter()
        .map(|position| position.coordinate.dimension().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_dimensions(requested: &DimensionSelection, included: &[String]) -> Vec<String> {
    if requested.mode() != DimensionSelectionMode::Only {
        return Vec::new();
    }

    let included = included.iter().cloned().collect::<BTreeSet<_>>();
    requested
        .dimensions()
        .difference(&included)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    #[test]
    fn near_includes_exact_cursor_positions_between_before_and_after() {
        let bundle = temporal_bundle(&[
            ("claim:one", "conversation", "conversation:main", 1),
            ("claim:two", "conversation", "conversation:main", 2),
            ("claim:three", "conversation", "conversation:main", 3),
        ]);
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Near,
            TemporalCursor::sequence(2).expect("sequence cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::only(["conversation"]))
        .with_window(TemporalWindow::new(1, 1));

        let result =
            TemporalMemoryTraversal::traverse(&bundle, &request).expect("near should traverse");
        let refs = result
            .entries()
            .iter()
            .map(|entry| entry.ref_id())
            .collect::<Vec<_>>();

        assert_eq!(refs, vec!["claim:one", "claim:two", "claim:three"]);
        assert_eq!(result.page().returned(), 3);
        assert_eq!(result.page().total(), 3);
        assert!(!result.page().has_more());
        assert!(result.missing().is_empty());
    }

    #[test]
    fn near_includes_exact_position_when_no_neighbors_exist() {
        let bundle = temporal_bundle(&[("claim:one", "decision", "decision:main", 1)]);
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Near,
            TemporalCursor::sequence(1).expect("sequence cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::only(["decision"]))
        .with_window(TemporalWindow::new(2, 2));

        let result =
            TemporalMemoryTraversal::traverse(&bundle, &request).expect("near should traverse");

        assert_eq!(result.entries()[0].ref_id(), "claim:one");
        assert_eq!(result.included_dimensions(), &["decision".to_string()]);
        assert!(result.missing_dimensions().is_empty());
        assert!(result.missing().is_empty());
    }

    #[test]
    fn forward_boundary_reports_no_entries_without_missing_positions() {
        let bundle = temporal_bundle(&[("claim:one", "decision", "decision:main", 1)]);
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::sequence(1).expect("sequence cursor should be valid"),
        )
        .with_dimensions(DimensionSelection::only(["decision"]));

        let result =
            TemporalMemoryTraversal::traverse(&bundle, &request).expect("forward should traverse");

        assert!(result.entries().is_empty());
        assert_eq!(result.page().returned(), 0);
        assert_eq!(result.page().total(), 0);
        assert!(!result.page().has_more());
        assert!(result.missing_dimensions().is_empty());
        assert!(result.missing().is_empty());
    }

    #[test]
    fn forward_reports_page_metadata_when_limited() {
        let bundle = temporal_bundle(&[
            ("claim:one", "decision", "decision:main", 1),
            ("claim:two", "decision", "decision:main", 2),
            ("claim:three", "decision", "decision:main", 3),
        ]);
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Forward,
            TemporalCursor::sequence(1).expect("sequence cursor should be valid"),
        )
        .with_limit_entries(1)
        .expect("limit should be valid");

        let result =
            TemporalMemoryTraversal::traverse(&bundle, &request).expect("forward should traverse");

        assert_eq!(result.entries()[0].ref_id(), "claim:two");
        assert_eq!(result.page().returned(), 1);
        assert_eq!(result.page().total(), 2);
        assert!(result.page().has_more());
        assert_eq!(result.page().next_cursor(), Some("claim:two"));
    }

    #[test]
    fn explicit_clock_axis_never_falls_back_to_another_clock() {
        let bundle = polytemporal_bundle();
        let cursor =
            || TemporalCursor::time("2026-04-12T11:30:00Z").expect("time cursor should be valid");

        let occurred = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(TemporalDirection::Goto, cursor())
                .with_axis(TemporalAxis::Occurred),
        )
        .expect("occurred axis should traverse");
        let observed = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(TemporalDirection::Goto, cursor())
                .with_axis(TemporalAxis::Observed),
        )
        .expect("observed axis should traverse");

        assert_eq!(
            occurred
                .entries()
                .iter()
                .map(TemporalEntry::ref_id)
                .collect::<Vec<_>>(),
            vec!["claim:one", "claim:two"]
        );
        assert_eq!(
            observed
                .entries()
                .iter()
                .map(TemporalEntry::ref_id)
                .collect::<Vec<_>>(),
            vec!["claim:two"]
        );
        assert_eq!(observed.axis(), TemporalAxis::Observed);
        assert_eq!(
            occurred.resolved_cursor().occurred_at(),
            Some("2026-04-12T11:30:00Z")
        );
        assert_eq!(occurred.resolved_cursor().observed_at(), None);
        assert_eq!(
            observed.resolved_cursor().observed_at(),
            Some("2026-04-12T11:30:00Z")
        );
        assert_eq!(observed.resolved_cursor().occurred_at(), None);

        let ingested = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(TemporalDirection::Goto, cursor())
                .with_axis(TemporalAxis::Ingested),
        )
        .expect("ingested axis should still resolve its cursor");
        assert_eq!(
            ingested.resolved_cursor().ingested_at(),
            Some("2026-04-12T11:30:00Z")
        );
        let validity = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(TemporalDirection::Goto, cursor())
                .with_axis(TemporalAxis::Validity),
        )
        .expect("validity axis should still resolve its cursor");
        assert_eq!(
            validity.resolved_cursor().valid_from(),
            Some("2026-04-12T11:30:00Z")
        );
    }

    #[test]
    fn validity_goto_projects_only_intervals_that_hold_at_the_cursor() {
        let bundle = validity_bundle(&[
            (
                "entry:expired",
                Some("2026-08-20T09:00:00Z"),
                Some("2026-08-20T12:00:00Z"),
            ),
            (
                "entry:lease",
                Some("2026-08-20T10:30:00Z"),
                Some("2026-08-20T12:00:00Z"),
            ),
            ("entry:current", Some("2026-08-20T12:00:00Z"), None),
            ("entry:open-start", None, Some("2026-08-20T14:00:00Z")),
            ("entry:future", Some("2027-01-01T00:00:00Z"), None),
        ]);
        let as_of = |instant: &str| {
            TemporalMemoryTraversal::traverse(
                &bundle,
                &TemporalTraversalRequest::new(
                    TemporalDirection::Goto,
                    TemporalCursor::time(instant).expect("time cursor"),
                )
                .with_axis(TemporalAxis::Validity),
            )
            .expect("validity projection")
            .entries()
            .iter()
            .map(TemporalEntry::ref_id)
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
        };

        assert_eq!(
            as_of("2026-08-20T11:00:00Z"),
            ["entry:expired", "entry:lease", "entry:open-start"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
        assert_eq!(
            as_of("2026-08-20T12:00:00Z"),
            ["entry:current", "entry:open-start"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            "valid_until is an exclusive interval boundary"
        );
        assert_eq!(
            as_of("2026-08-20T13:00:00Z"),
            ["entry:current", "entry:open-start"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
    }

    #[test]
    fn validity_time_moves_exclude_intervals_that_ended_at_the_cursor() {
        let bundle = validity_bundle(&[
            (
                "entry:expired",
                Some("2026-08-20T09:00:00Z"),
                Some("2026-08-20T12:00:00Z"),
            ),
            (
                "entry:lease",
                Some("2026-08-20T10:30:00Z"),
                Some("2026-08-20T12:00:00Z"),
            ),
            ("entry:current", Some("2026-08-20T12:00:00Z"), None),
            ("entry:future", Some("2027-01-01T00:00:00Z"), None),
        ]);
        let move_refs = |direction| {
            TemporalMemoryTraversal::traverse(
                &bundle,
                &TemporalTraversalRequest::new(
                    direction,
                    TemporalCursor::time("2026-08-20T13:00:00Z").expect("time cursor"),
                )
                .with_axis(TemporalAxis::Validity),
            )
            .expect("validity move")
            .entries()
            .iter()
            .map(TemporalEntry::ref_id)
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
        };
        let refs = |values: &[&str]| {
            values
                .iter()
                .map(|value| (*value).to_string())
                .collect::<BTreeSet<_>>()
        };

        assert_eq!(move_refs(TemporalDirection::Goto), refs(&["entry:current"]));
        assert_eq!(
            move_refs(TemporalDirection::Rewind),
            refs(&["entry:current"])
        );
        assert_eq!(
            move_refs(TemporalDirection::Near),
            refs(&["entry:current", "entry:future"])
        );
        assert_eq!(
            move_refs(TemporalDirection::Forward),
            refs(&["entry:future"])
        );
    }

    #[test]
    fn ref_cursor_without_requested_clock_returns_an_empty_proven_absence() {
        let bundle = temporal_bundle(&[("claim:one", "decision", "decision:main", 1)]);
        let request = TemporalTraversalRequest::new(
            TemporalDirection::Rewind,
            TemporalCursor::ref_id("claim:one").expect("ref cursor"),
        )
        .with_axis(TemporalAxis::Occurred);

        let result =
            TemporalMemoryTraversal::traverse(&bundle, &request).expect("ref should resolve");

        assert!(result.entries().is_empty());
        assert_eq!(result.resolved_cursor().sequence(), Some(1));
        assert_eq!(result.missing(), &["temporal_positions".to_string()]);
        assert_eq!(result.page().returned(), 0);
        assert_eq!(result.page().total(), 0);
    }

    #[test]
    fn rewind_preserves_sequence_order_for_timestamp_ties_at_either_boundary() {
        let bundle = clocked_temporal_bundle(&[
            ("entry-1", "2026-08-27T10:00:00Z", 1),
            ("entry-2", "2026-08-27T11:00:00Z", 2),
            ("entry-z", "2026-08-27T12:00:00Z", 3),
            ("entry-a", "2026-08-27T12:00:00Z", 4),
            ("entry-5", "2026-08-27T13:00:00Z", 5),
            ("entry-6", "2026-08-27T14:00:00Z", 6),
        ]);
        let rewind = |cursor: &str| {
            TemporalMemoryTraversal::traverse(
                &bundle,
                &TemporalTraversalRequest::new(
                    TemporalDirection::Rewind,
                    TemporalCursor::time(cursor).expect("time cursor"),
                )
                .with_limit_entries(3)
                .expect("limit"),
            )
            .expect("rewind")
            .entries()
            .iter()
            .map(TemporalEntry::ref_id)
            .map(str::to_string)
            .collect::<Vec<_>>()
        };

        assert_eq!(
            rewind("2026-08-27T13:00:00Z"),
            ["entry-2", "entry-z", "entry-a"]
        );
        assert_eq!(
            rewind("2026-08-27T14:00:00Z"),
            ["entry-z", "entry-a", "entry-5"]
        );
    }

    #[test]
    fn bounded_pages_limit_whole_entries_with_multiple_coordinates() {
        let bundle = clocked_temporal_bundle(&[
            ("entry-1", "2026-08-27T10:00:00Z", 1),
            ("entry-2", "2026-08-27T11:00:00Z", 2),
            ("entry-z", "2026-08-27T12:00:00Z", 3),
            ("entry-a", "2026-08-27T12:00:00Z", 4),
            ("entry-5", "2026-08-27T13:00:00Z", 5),
            ("entry-6", "2026-08-27T14:00:00Z", 6),
        ]);
        let traverse = |direction, cursor: &str| {
            TemporalMemoryTraversal::traverse(
                &bundle,
                &TemporalTraversalRequest::new(
                    direction,
                    TemporalCursor::time(cursor).expect("time cursor"),
                )
                .with_axis(TemporalAxis::Observed)
                .with_limit_entries(3)
                .expect("limit"),
            )
            .expect("traversal")
        };

        let rewind = traverse(TemporalDirection::Rewind, "2026-08-27T14:00:00Z");
        let forward = traverse(TemporalDirection::Forward, "2026-08-27T11:00:00Z");
        for result in [&rewind, &forward] {
            assert_eq!(result.page().returned(), 3);
            assert_eq!(
                result
                    .entries()
                    .iter()
                    .map(TemporalEntry::ref_id)
                    .collect::<Vec<_>>(),
                ["entry-z", "entry-a", "entry-5"]
            );
            assert!(
                result
                    .entries()
                    .iter()
                    .all(|entry| entry.coordinates().len() == 2)
            );
        }
        assert_eq!(rewind.page().total(), 5);
        assert_eq!(forward.page().total(), 4);
    }

    #[test]
    fn ref_cursors_and_page_continuations_preserve_timestamp_ties() {
        let bundle = clocked_temporal_bundle(&[
            ("entry-1", "2026-08-27T10:00:00Z", 1),
            ("entry-2", "2026-08-27T11:00:00Z", 2),
            ("entry-z", "2026-08-27T12:00:00Z", 3),
            ("entry-a", "2026-08-27T12:00:00Z", 4),
            ("entry-5", "2026-08-27T13:00:00Z", 5),
            ("entry-6", "2026-08-27T14:00:00Z", 6),
        ]);
        let traverse = |direction, cursor, limit| {
            TemporalMemoryTraversal::traverse(
                &bundle,
                &TemporalTraversalRequest::new(direction, cursor)
                    .with_axis(TemporalAxis::Observed)
                    .with_limit_entries(limit)
                    .expect("limit"),
            )
            .expect("traversal")
        };

        let rewind = traverse(
            TemporalDirection::Rewind,
            TemporalCursor::ref_id("entry-a").expect("ref cursor"),
            10,
        );
        assert_eq!(
            rewind
                .entries()
                .iter()
                .map(TemporalEntry::ref_id)
                .collect::<Vec<_>>(),
            ["entry-1", "entry-2", "entry-z"]
        );
        let forward = traverse(
            TemporalDirection::Forward,
            TemporalCursor::ref_id("entry-z").expect("ref cursor"),
            10,
        );
        assert_eq!(
            forward
                .entries()
                .iter()
                .map(TemporalEntry::ref_id)
                .collect::<Vec<_>>(),
            ["entry-a", "entry-5", "entry-6"]
        );

        let mut cursor = TemporalCursor::time("2026-08-27T14:00:00Z").expect("time cursor");
        let mut seen = BTreeSet::new();
        loop {
            let page = traverse(TemporalDirection::Rewind, cursor, 2);
            for entry in page.entries() {
                assert!(
                    seen.insert(entry.ref_id().to_string()),
                    "pagination repeated {}",
                    entry.ref_id()
                );
            }
            let Some(next) = page.page().next_cursor() else {
                break;
            };
            cursor = TemporalCursor::ref_id(next).expect("page ref cursor");
        }
        assert_eq!(
            seen,
            ["entry-1", "entry-2", "entry-z", "entry-a", "entry-5"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
    }

    #[test]
    fn sequence_ties_are_broken_by_the_recorded_clock_before_ref() {
        let root = node("question:a", "question", "Question A");
        let scope = node("timeline:main", "memory_dimension", "timeline:main");
        let nodes = vec![
            scope,
            node("decision-new", "decision", "new"),
            node("decision-old", "decision", "old"),
        ];
        let coordinate = |target: &str, observed_at: &str| {
            BundleRelationship::new(
                "timeline:main",
                target,
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("timeline")
                    .with_scope_id("timeline:main")
                    .with_observed_at(observed_at)
                    .with_sequence(1),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("question:a").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            root,
            nodes,
            vec![
                coordinate("decision-new", "2026-08-27T16:55:40Z"),
                coordinate("decision-old", "2026-08-27T16:55:00Z"),
            ],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");

        let result = TemporalMemoryTraversal::traverse(
            &bundle,
            &TemporalTraversalRequest::new(
                TemporalDirection::Goto,
                TemporalCursor::sequence(1).expect("sequence"),
            ),
        )
        .expect("traversal");

        assert_eq!(
            result
                .entries()
                .iter()
                .map(TemporalEntry::ref_id)
                .collect::<Vec<_>>(),
            vec!["decision-old", "decision-new"]
        );
    }

    fn polytemporal_bundle() -> KmpBundle {
        let root = node("question:a", "question", "Question A");
        let scope = node("timeline:main", "memory_dimension", "timeline:main");
        let entries = [
            ("claim:one", "2026-04-12T10:00:00Z", "2026-04-12T12:00:00Z"),
            ("claim:two", "2026-04-12T11:00:00Z", "2026-04-12T11:00:00Z"),
        ];
        let mut nodes = vec![scope];
        nodes.extend(
            entries
                .iter()
                .map(|(ref_id, _, _)| node(ref_id, "claim", ref_id)),
        );
        let relationships = entries
            .iter()
            .enumerate()
            .map(|(index, (ref_id, occurred_at, observed_at))| {
                BundleRelationship::new(
                    "timeline:main",
                    *ref_id,
                    "contains_entry",
                    RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_dimension("timeline")
                        .with_scope_id("timeline:main")
                        .with_occurred_at(*occurred_at)
                        .with_observed_at(*observed_at)
                        .with_sequence((index + 1) as u32),
                )
            })
            .collect();

        KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("temporal-reader").expect("role should be valid"),
            root,
            nodes,
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("polytemporal bundle should be valid")
    }

    fn temporal_bundle(entries: &[(&str, &str, &str, u32)]) -> KmpBundle {
        let mut nodes = BTreeMap::new();
        for (ref_id, _, scope_id, _) in entries {
            nodes.insert(
                (*scope_id).to_string(),
                node(scope_id, "memory_dimension", scope_id),
            );
            nodes.insert((*ref_id).to_string(), node(ref_id, "claim", ref_id));
        }

        KmpBundle::new(
            CaseId::new("question:a").expect("case id should be valid"),
            Role::new("temporal-reader").expect("role should be valid"),
            node("question:a", "question", "Question A"),
            nodes.into_values().collect(),
            entries
                .iter()
                .map(|(ref_id, dimension, scope_id, sequence)| {
                    BundleRelationship::new(
                        *scope_id,
                        *ref_id,
                        "contains_entry",
                        RelationExplanation::new(RelationSemanticClass::Structural)
                            .with_dimension(*dimension)
                            .with_scope_id(*scope_id)
                            .with_sequence(*sequence),
                    )
                })
                .collect(),
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("test bundle should be valid")
    }

    fn clocked_temporal_bundle(entries: &[(&str, &str, u32)]) -> KmpBundle {
        let dimensions = [("process", "process:main"), ("task", "task:main")];
        let mut nodes = dimensions
            .iter()
            .map(|(_, scope_id)| node(scope_id, "memory_dimension", scope_id))
            .collect::<Vec<_>>();
        nodes.extend(
            entries
                .iter()
                .map(|(ref_id, _, _)| node(ref_id, "claim", ref_id)),
        );
        let relationships = entries
            .iter()
            .flat_map(|(ref_id, observed_at, sequence)| {
                dimensions.iter().map(move |(dimension, scope_id)| {
                    BundleRelationship::new(
                        *scope_id,
                        *ref_id,
                        "contains_entry",
                        RelationExplanation::new(RelationSemanticClass::Structural)
                            .with_dimension(*dimension)
                            .with_scope_id(*scope_id)
                            .with_observed_at(*observed_at)
                            .with_sequence(*sequence),
                    )
                })
            })
            .collect();

        KmpBundle::new(
            CaseId::new("question:a").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("question:a", "question", "Question A"),
            nodes,
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("clocked temporal bundle")
    }

    fn validity_bundle(entries: &[(&str, Option<&str>, Option<&str>)]) -> KmpBundle {
        let mut nodes = vec![node("validity:main", "memory_dimension", "validity:main")];
        nodes.extend(
            entries
                .iter()
                .map(|(ref_id, _, _)| node(ref_id, "claim", ref_id)),
        );
        let relationships = entries
            .iter()
            .map(|(ref_id, valid_from, valid_until)| {
                BundleRelationship::new(
                    "validity:main",
                    *ref_id,
                    "contains_entry",
                    RelationExplanation::new(RelationSemanticClass::Structural)
                        .with_dimension("validity")
                        .with_scope_id("validity:main")
                        .with_optional_valid_from(valid_from.map(ToString::to_string))
                        .with_optional_valid_until(valid_until.map(ToString::to_string)),
                )
            })
            .collect();

        KmpBundle::new(
            CaseId::new("question:a").expect("case id"),
            Role::new("temporal-reader").expect("role"),
            node("question:a", "question", "Question A"),
            nodes,
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("validity bundle")
    }

    fn node(node_id: &str, kind: &str, title: &str) -> BundleNode {
        BundleNode::new(
            node_id,
            kind,
            title,
            title,
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }
}
