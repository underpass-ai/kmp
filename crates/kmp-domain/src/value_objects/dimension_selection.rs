use std::collections::BTreeSet;

use crate::MemoryDimensionIdentity;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionSelection {
    mode: DimensionSelectionMode,
    dimensions: BTreeSet<String>,
    scope_mode: DimensionScopeMode,
    abouts: BTreeSet<String>,
    scope_ids: BTreeSet<String>,
}

impl DimensionSelection {
    pub fn all() -> Self {
        Self {
            mode: DimensionSelectionMode::All,
            dimensions: BTreeSet::new(),
            scope_mode: DimensionScopeMode::CurrentAbout,
            abouts: BTreeSet::new(),
            scope_ids: BTreeSet::new(),
        }
    }

    pub fn only(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: DimensionSelectionMode::Only,
            dimensions: normalize_dimensions(values),
            scope_mode: DimensionScopeMode::CurrentAbout,
            abouts: BTreeSet::new(),
            scope_ids: BTreeSet::new(),
        }
    }

    pub fn except(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: DimensionSelectionMode::Except,
            dimensions: normalize_dimensions(values),
            scope_mode: DimensionScopeMode::CurrentAbout,
            abouts: BTreeSet::new(),
            scope_ids: BTreeSet::new(),
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

    pub fn with_scope_ids(
        mut self,
        scope_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.scope_ids = normalize_dimensions(scope_ids);
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
    use super::DimensionSelection;

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
}
