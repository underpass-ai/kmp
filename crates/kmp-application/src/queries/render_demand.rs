/// How much of the rendered prompt a read's caller will consume.
///
/// Rendering tokenizes with cl100k, and most of that work is measurement: the
/// token counts of every section and tier, and the quality metrics that
/// tokenize a raw dump of the whole bundle. A caller that returns the prompt
/// needs all of it; a caller that only projects the structured bundle needs
/// part of it or none. The demand is stated by the caller, never guessed, and
/// the result records it in [`RenderedContext::demand`](super::RenderedContext::demand)
/// so nothing downstream mistakes an unmeasured render for a measured one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderDemand {
    /// The complete render: content, sections, tiers, every token count and
    /// the quality metrics. What an API that returns the prompt, and a
    /// quality observer, read.
    #[default]
    Measured,
    /// The rendered text a projection reads: content, sections and tiers,
    /// selected under the same token budget, so they are byte-identical to a
    /// measured render. Tokens are counted only where a budget decision needs
    /// them; counts no decision needed read `0`, and the quality metrics are
    /// [`BundleQualityMetrics::unmeasured`](kmp_domain::BundleQualityMetrics::unmeasured).
    Structure,
    /// Nothing reads the render: no text, no tokens, no metrics.
    Skip,
}

impl RenderDemand {
    /// Whether the token counts and quality metrics are measured.
    pub fn is_measured(self) -> bool {
        self == Self::Measured
    }

    /// Whether any rendered text is produced.
    pub fn renders_text(self) -> bool {
        self != Self::Skip
    }
}

#[cfg(test)]
mod tests {
    use super::RenderDemand;

    #[test]
    fn the_default_demand_is_the_complete_measured_render() {
        assert_eq!(RenderDemand::default(), RenderDemand::Measured);
        assert!(RenderDemand::Measured.is_measured());
        assert!(RenderDemand::Measured.renders_text());
    }

    #[test]
    fn structure_renders_text_without_measuring_and_skip_renders_nothing() {
        assert!(!RenderDemand::Structure.is_measured());
        assert!(RenderDemand::Structure.renders_text());
        assert!(!RenderDemand::Skip.is_measured());
        assert!(!RenderDemand::Skip.renders_text());
    }
}
