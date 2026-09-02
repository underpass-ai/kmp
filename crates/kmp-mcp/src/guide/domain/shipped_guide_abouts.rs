/// The abouts the shipped guide occupies in whatever store it is synced into.
///
/// The guide is not a project's memory. It is content that travels with the
/// release, regenerated from `plugins/kmp/guide/` every time setup runs, and a
/// project's committed bundle has never carried it. Anything that compares a
/// store against that bundle, or writes it, has to be able to say which events
/// are the guide's — and it has to say it once, in a place both the guide
/// context and the lifecycle context can read, rather than repeating two string
/// literals wherever the question comes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippedGuideAbouts;

impl ShippedGuideAbouts {
    /// The human guide ChronoLoom opens.
    pub const HUMAN: &'static str = "guide:kmp";
    /// The agent guide the verbs are learned from.
    pub const AGENT: &'static str = "guide:kmp-agent";

    /// Both, in the order the guide bundle header records them.
    pub fn all() -> [&'static str; 2] {
        [Self::HUMAN, Self::AGENT]
    }

    pub fn owned() -> Vec<String> {
        Self::all().map(str::to_string).to_vec()
    }

    /// Whether an about names shipped guide content rather than authored
    /// memory. Abouts are opaque: this is exact matching, with no trimming,
    /// case folding or prefix expansion.
    pub fn contains(about: &str) -> bool {
        Self::all().contains(&about)
    }
}

#[cfg(test)]
mod tests {
    use super::ShippedGuideAbouts;

    #[test]
    fn both_guide_abouts_are_recognised_and_nothing_else_is() {
        assert!(ShippedGuideAbouts::contains("guide:kmp"));
        assert!(ShippedGuideAbouts::contains("guide:kmp-agent"));
        assert!(!ShippedGuideAbouts::contains("project:kmp"));
    }

    #[test]
    fn matching_is_exact_because_abouts_are_opaque() {
        assert!(!ShippedGuideAbouts::contains(" guide:kmp"));
        assert!(!ShippedGuideAbouts::contains("guide:kmp "));
        assert!(!ShippedGuideAbouts::contains("GUIDE:KMP"));
        assert!(!ShippedGuideAbouts::contains("guide:kmp:extra"));
    }

    #[test]
    fn owned_carries_the_same_two_in_the_same_order() {
        assert_eq!(
            ShippedGuideAbouts::owned(),
            vec!["guide:kmp".to_string(), "guide:kmp-agent".to_string()]
        );
    }
}
