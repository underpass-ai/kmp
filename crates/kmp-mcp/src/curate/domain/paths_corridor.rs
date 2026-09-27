/// Whether a `kmp_curate` path search with a goal asks the judge about the
/// corridor between the ends, with each fact's next step offered among at
/// most eight neighbours (DESIGN L7), or about the facts the graph links
/// near the ends with every other fact as an option, as before.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PathsCorridor {
    #[default]
    On,
    Off,
}

impl PathsCorridor {
    /// The setting a store names (`on`, `off`), or `None` for any other name.
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub(crate) fn is_on(self) -> bool {
        self == Self::On
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_names_the_corridor_on_or_off() {
        assert_eq!(PathsCorridor::named("on"), Some(PathsCorridor::On));
        assert_eq!(PathsCorridor::named("off"), Some(PathsCorridor::Off));
        assert_eq!(PathsCorridor::named("near"), None);
        assert!(PathsCorridor::default().is_on());
    }
}
