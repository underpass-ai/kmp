//! The bounded additional contexts a shared scene compares.

use super::{AboutId, ViewError};

/// Additional about planes beside the primary about. Stable order, no repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutLayers(Vec<AboutId>);

impl AboutLayers {
    /// At most five additional reads per projection: six planes including primary.
    pub const LIMIT: usize = 5;

    /// Validates identifiers and the explicit read budget. Empty means primary only.
    pub fn new(names: impl IntoIterator<Item = String>) -> Result<Self, ViewError> {
        let mut abouts = Vec::new();
        for name in names {
            if name.trim().is_empty() {
                return Err(ViewError::Invalid(
                    "a layer needs an about identifier".into(),
                ));
            }
            let about = AboutId::new(name);
            if !abouts.contains(&about) {
                abouts.push(about);
            }
        }
        if abouts.len() > Self::LIMIT {
            return Err(ViewError::Invalid(
                "at most five additional about layers are allowed".into(),
            ));
        }
        Ok(Self(abouts))
    }

    /// The contexts in their stable display order.
    pub fn abouts(&self) -> &[AboutId] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layers_are_explicit_bounded_and_stably_deduplicated() {
        let layers =
            AboutLayers::new(["b", "a", "b"].map(str::to_string)).expect("valid bounded layers");
        assert_eq!(
            layers
                .abouts()
                .iter()
                .map(AboutId::as_str)
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(AboutLayers::new([" ".into()]).is_err());
        assert!(AboutLayers::new((0..6).map(|n| format!("about:{n}"))).is_err());
        assert!(
            AboutLayers::new(Vec::new())
                .expect("valid bounded layers")
                .abouts()
                .is_empty()
        );
    }
}
