use std::collections::BTreeSet;

use crate::MemoryDimensionIdentity;
use crate::value_objects::entry_labels::EntryLabels;
use crate::value_objects::label_selector::{LabelSelector, LabelSelectorOperator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionSelectionMode {
    All,
    Only,
    Except,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionScopeMode {
    CurrentAbout,
    Abouts,
    AllAbouts,
}

/// What a read keeps of the coordinates it meets.
///
/// Two filters, two grains. `mode` with `dimensions` and `scope_ids` read
/// one coordinate at a time: a coordinate passes or not, and an entry stays
/// when one of its coordinates passes (`includes_coordinate`). `selectors`
/// read the whole entry as a map of labels (`admits`): `notexists task`
/// keeps only the entries with no `task` label, where `except task` keeps
/// every entry that also stands in a process. Both are hard filters; neither
/// scores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionSelection {
    mode: DimensionSelectionMode,
    dimensions: BTreeSet<String>,
    scope_mode: DimensionScopeMode,
    abouts: BTreeSet<String>,
    scope_ids: BTreeSet<String>,
    selectors: Vec<LabelSelector>,
}

impl DimensionSelection {
    pub fn all() -> Self {
        Self {
            mode: DimensionSelectionMode::All,
            dimensions: BTreeSet::new(),
            scope_mode: DimensionScopeMode::CurrentAbout,
            abouts: BTreeSet::new(),
            scope_ids: BTreeSet::new(),
            selectors: Vec::new(),
        }
    }

    pub fn only(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: DimensionSelectionMode::Only,
            dimensions: normalize_dimensions(values),
            ..Self::all()
        }
    }

    pub fn except(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: DimensionSelectionMode::Except,
            dimensions: normalize_dimensions(values),
            ..Self::all()
        }
    }

    pub fn mode(&self) -> DimensionSelectionMode {
        self.mode
    }

    pub fn dimensions(&self) -> &BTreeSet<String> {
        &self.dimensions
    }

    pub fn scope_mode(&self) -> DimensionScopeMode {
        self.scope_mode
    }

    pub fn abouts(&self) -> &BTreeSet<String> {
        &self.abouts
    }

    pub fn scope_ids(&self) -> &BTreeSet<String> {
        &self.scope_ids
    }

    /// The label predicates every kept entry satisfies, in a stable order.
    pub fn selectors(&self) -> &[LabelSelector] {
        &self.selectors
    }

    pub fn with_scope_ids(
        mut self,
        scope_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.scope_ids = normalize_dimensions(scope_ids);
        self
    }

    /// Selectors are conjoined; duplicates collapse and the order is the
    /// selectors' own, so two equal selections hash alike in a cursor.
    pub fn with_selectors(mut self, selectors: impl IntoIterator<Item = LabelSelector>) -> Self {
        self.selectors = selectors
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self
    }

    pub fn with_current_about_scope(mut self) -> Self {
        self.scope_mode = DimensionScopeMode::CurrentAbout;
        self.abouts.clear();
        self
    }

    pub fn with_about_scope(mut self, abouts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.scope_mode = DimensionScopeMode::Abouts;
        self.abouts = normalize_dimensions(abouts);
        self
    }

    pub fn with_all_about_scope(mut self) -> Self {
        self.scope_mode = DimensionScopeMode::AllAbouts;
        self.abouts.clear();
        self
    }

    pub fn resolve_current_about(&self, current_about: &str) -> Self {
        if self.scope_mode != DimensionScopeMode::CurrentAbout {
            return self.clone();
        }
        self.clone().with_about_scope([current_about.to_string()])
    }

    pub fn includes(&self, dimension: &str) -> bool {
        self.includes_dimension(dimension)
    }

    pub fn includes_dimension(&self, dimension: &str) -> bool {
        match self.mode {
            DimensionSelectionMode::All => true,
            DimensionSelectionMode::Only => self.dimensions.contains(dimension),
            DimensionSelectionMode::Except => !self.dimensions.contains(dimension),
        }
    }

    pub fn includes_coordinate(&self, dimension: &str, scope_id: &str) -> bool {
        self.includes_dimension(dimension)
            && self.includes_scope(scope_id)
            && self.includes_dimension_scope(scope_id)
    }

    pub fn includes_scope(&self, scope_id: &str) -> bool {
        match self.scope_mode {
            DimensionScopeMode::AllAbouts => true,
            DimensionScopeMode::CurrentAbout => true,
            DimensionScopeMode::Abouts => MemoryDimensionIdentity::parse(scope_id)
                .map(|identity| self.abouts.contains(identity.about()))
                .unwrap_or(false),
        }
    }

    pub fn includes_dimension_scope(&self, scope_id: &str) -> bool {
        if self.scope_ids.is_empty() {
            return true;
        }
        let scope_id = scope_id.trim();
        self.scope_ids.contains(scope_id)
            || MemoryDimensionIdentity::parse(scope_id)
                .map(|identity| self.scope_ids.contains(identity.dimension_id()))
                .unwrap_or(false)
    }

    /// Does an entry standing in these labels pass every selector? With no
    /// selectors everything does; the coordinate filters above still apply.
    pub fn admits(&self, labels: &EntryLabels) -> bool {
        self.selectors
            .iter()
            .all(|selector| selector.admits(labels))
    }

    pub fn has_selectors(&self) -> bool {
        !self.selectors.is_empty()
    }

    /// The labels the positive selectors name, as an about index reads
    /// them: a key for `exists`, values for `in`. A negative selector names
    /// nothing an index could pick by, so it narrows nothing here and the
    /// filter does the excluding.
    pub fn positive_selector_ids(&self) -> Vec<String> {
        self.selectors
            .iter()
            .flat_map(|selector| match selector.operator() {
                LabelSelectorOperator::Exists => vec![selector.key().to_string()],
                LabelSelectorOperator::In => selector.values().iter().cloned().collect(),
                LabelSelectorOperator::NotIn | LabelSelectorOperator::NotExists => Vec::new(),
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl Default for DimensionSelection {
    fn default() -> Self {
        Self::all()
    }
}

fn normalize_dimensions(values: impl IntoIterator<Item = impl Into<String>>) -> BTreeSet<String> {
    values
        .into_iter()
        .map(Into::into)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DimensionSelection, EntryLabels, LabelSelector, LabelSelectorOperator};

    #[test]
    fn selection_filters_dimensions() {
        let only = DimensionSelection::only(["conversation", "entity", " "]);
        assert!(only.includes("conversation"));
        assert!(!only.includes("benchmark_record"));

        let except = DimensionSelection::except(["entity"]);
        assert!(except.includes("conversation"));
        assert!(!except.includes("entity"));
    }

    #[test]
    fn selection_filters_about_scope_when_resolved() {
        let current = DimensionSelection::only(["timeline"]).resolve_current_about("question:a");
        assert!(current.includes_coordinate("timeline", "about:question:a:dimension:timeline"));
        assert!(!current.includes_coordinate("timeline", "about:question:b:dimension:timeline"));

        let all = DimensionSelection::only(["timeline"]).with_all_about_scope();
        assert!(all.includes_coordinate("timeline", "about:question:b:dimension:timeline"));
    }

    #[test]
    fn selection_filters_exact_dimension_scope_ids() {
        let local = DimensionSelection::only(["conversation"])
            .resolve_current_about("question:a")
            .with_scope_ids(["conversation:alpha"]);
        assert!(local.includes_coordinate(
            "conversation",
            "about:question:a:dimension:conversation:alpha"
        ));
        assert!(!local.includes_coordinate(
            "conversation",
            "about:question:a:dimension:conversation:beta"
        ));
        assert!(
            !local.includes_coordinate("topic", "about:question:a:dimension:conversation:alpha")
        );

        let namespaced = DimensionSelection::all()
            .with_scope_ids(["about:question:a:dimension:conversation:alpha"]);
        assert!(namespaced.includes_coordinate(
            "conversation",
            "about:question:a:dimension:conversation:alpha"
        ));
        assert!(!namespaced.includes_coordinate(
            "conversation",
            "about:question:b:dimension:conversation:alpha"
        ));
    }

    #[test]
    fn selectors_read_the_whole_entry_and_conjoin() {
        let selection = DimensionSelection::all().with_selectors([
            LabelSelector::new("env", LabelSelectorOperator::In, ["prod"]).expect("selector"),
            LabelSelector::new(
                "task",
                LabelSelectorOperator::NotExists,
                Vec::<String>::new(),
            )
            .expect("selector"),
        ]);
        let prod_without_task =
            EntryLabels::from_pairs([("env", "prod"), ("agentic_process", "p-1")]);
        let prod_with_task = EntryLabels::from_pairs([("env", "prod"), ("task", "t-1")]);
        let dev = EntryLabels::from_pairs([("env", "dev")]);
        assert!(selection.admits(&prod_without_task));
        assert!(!selection.admits(&prod_with_task));
        assert!(!selection.admits(&dev));
        assert!(DimensionSelection::all().admits(&dev));
    }

    #[test]
    fn except_and_notexists_are_not_duals() {
        // `except task` keeps an entry through its other coordinates;
        // `notexists task` keeps only entries with no task label at all.
        let except = DimensionSelection::except(["task"]);
        assert!(except.includes_coordinate("agentic_process", "about:a:dimension:p-1"));
        let entry = EntryLabels::from_pairs([("agentic_process", "p-1"), ("task", "t-1")]);
        let absent = DimensionSelection::all().with_selectors([LabelSelector::new(
            "task",
            LabelSelectorOperator::NotExists,
            Vec::<String>::new(),
        )
        .expect("selector")]);
        assert!(!absent.admits(&entry));
    }

    #[test]
    fn positive_selectors_name_what_an_index_can_pick_by() {
        let selection = DimensionSelection::all().with_selectors([
            LabelSelector::new(
                "incident",
                LabelSelectorOperator::Exists,
                Vec::<String>::new(),
            )
            .expect("selector"),
            LabelSelector::new("env", LabelSelectorOperator::In, ["prod", "staging"])
                .expect("selector"),
            LabelSelector::new("customer", LabelSelectorOperator::NotIn, ["acme"])
                .expect("selector"),
        ]);
        assert_eq!(
            selection.positive_selector_ids(),
            vec![
                "incident".to_string(),
                "prod".to_string(),
                "staging".to_string()
            ]
        );
        assert!(
            DimensionSelection::all()
                .with_selectors([LabelSelector::new(
                    "customer",
                    LabelSelectorOperator::NotIn,
                    ["acme"]
                )
                .expect("selector")])
                .positive_selector_ids()
                .is_empty()
        );
    }

    #[test]
    fn duplicate_selectors_collapse_into_a_stable_order() {
        let first =
            LabelSelector::new("env", LabelSelectorOperator::In, ["prod"]).expect("selector");
        let second =
            LabelSelector::new("task", LabelSelectorOperator::Exists, Vec::<String>::new())
                .expect("selector");
        let selection = DimensionSelection::all().with_selectors([
            second.clone(),
            first.clone(),
            first.clone(),
        ]);
        assert_eq!(selection.selectors(), &[first, second]);
    }
}
