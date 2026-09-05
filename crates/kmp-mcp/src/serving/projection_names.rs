use kmp_viewer::LabelSelectorDto;

/// The projections one view intent names, counted for honoring.
#[derive(Debug, Default)]
pub(crate) struct ProjectionNames {
    pub(crate) dimensions: Vec<String>,
    pub(crate) overlays: Vec<String>,
    /// The label predicates, whose keys and `in` values the catalogue must
    /// hold.
    pub(crate) labels: Vec<LabelSelectorDto>,
}
