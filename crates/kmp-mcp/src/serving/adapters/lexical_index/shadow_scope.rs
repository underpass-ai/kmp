/// How an ask's read relates to what the lexical sidecar indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowScope {
    /// One about at the frontier with no dimensions: the sidecar indexes
    /// exactly this read at the default depth, and at any greater depth
    /// while nothing lies past the default one (`deeper`).
    Indexed { deeper: bool },
    /// The same, narrowed by dimensions within the about: a subset of it.
    Selection { deeper: bool },
    /// A shallower read, another clock or another set of abouts.
    Other,
}

impl ShadowScope {
    pub(crate) fn of(query: &kmp_application::memory::AskMemoryQuery, depth: u8) -> Self {
        let read = kmp_application::queries::clamp_native_graph_traversal_depth(query.depth);
        let plain =
            read >= u32::from(depth) && query.temporal == kmp_domain::TemporalSelection::Frontier;
        let deeper = read > u32::from(depth);
        // An ask of the about itself, however its scope is spelled.
        let about = &query.about;
        let dimensions = query.dimensions.resolve_current_about(about);
        let only_this_about = dimensions.scope_mode() == kmp_domain::DimensionScopeMode::Abouts
            && dimensions.abouts().len() == 1
            && dimensions.abouts().contains(about);
        if !plain || !only_this_about {
            Self::Other
        } else if dimensions
            == kmp_domain::DimensionSelection::default().resolve_current_about(about)
        {
            Self::Indexed { deeper }
        } else {
            Self::Selection { deeper }
        }
    }

    /// Whether the ask reads deeper than the sidecar indexes.
    pub(crate) fn deeper(self) -> bool {
        matches!(
            self,
            Self::Indexed { deeper: true } | Self::Selection { deeper: true }
        )
    }
}
