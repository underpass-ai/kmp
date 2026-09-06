//! One label predicate the loom keeps.

use crate::view::domain::label_operator::LabelOperator;
use crate::view::domain::view_error::ViewError;

/// A predicate over the labels an entry stands in, as a view holds it: a
/// key, one of the four operators, and the values the operator compares.
/// It is the shape `dimensions.selectors` takes on every kernel read, so an
/// agent learns one grammar; the kernel applies it as a hard filter.
///
/// Only well-formed predicates become state: a key is never empty, `in` and
/// `notin` name at least one value, `exists` and `notexists` name none.
/// Values are kept sorted and deduplicated, so two selections that mean the
/// same thing compare equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSelection {
    key: String,
    operator: LabelOperator,
    values: Vec<String>,
}

impl LabelSelection {
    /// Builds a predicate, refusing the shapes that mean nothing.
    pub fn new(
        key: impl Into<String>,
        operator: LabelOperator,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ViewError> {
        let key = key.into().trim().to_string();
        if key.is_empty() {
            return Err(ViewError::Invalid(
                "a label selector names a key; an empty one selects nothing".to_string(),
            ));
        }
        let mut values = values
            .into_iter()
            .map(Into::into)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        values.sort();
        values.dedup();
        if operator.takes_values() && values.is_empty() {
            return Err(ViewError::Invalid(format!(
                "`{key} {}` needs at least one value",
                operator.as_str()
            )));
        }
        if !operator.takes_values() && !values.is_empty() {
            return Err(ViewError::Invalid(format!(
                "`{key} {}` takes no values",
                operator.as_str()
            )));
        }
        Ok(Self {
            key,
            operator,
            values,
        })
    }

    /// The label key the predicate reads.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// How it reads it.
    pub fn operator(&self) -> LabelOperator {
        self.operator
    }

    /// The values it compares, sorted; empty for `exists` and `notexists`.
    pub fn values(&self) -> &[String] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_keeps_its_values_sorted_and_unique() {
        let selection = LabelSelection::new(" task ", LabelOperator::In, ["b", "a", "b", " "])
            .expect("a well-formed selector");
        assert_eq!(selection.key(), "task");
        assert_eq!(selection.values(), &["a", "b"]);
        assert_eq!(
            selection,
            LabelSelection::new("task", LabelOperator::In, ["a", "b"]).expect("same meaning")
        );
    }

    #[test]
    fn the_shapes_that_mean_nothing_are_refused() {
        let empty_key = LabelSelection::new("", LabelOperator::Exists, Vec::<String>::new());
        assert!(matches!(empty_key, Err(ViewError::Invalid(_))));
        let no_values = LabelSelection::new("task", LabelOperator::In, Vec::<String>::new());
        assert!(matches!(no_values, Err(ViewError::Invalid(_))));
        let stray_values = LabelSelection::new("task", LabelOperator::NotExists, ["a"]);
        assert!(matches!(stray_values, Err(ViewError::Invalid(_))));
        assert!(
            LabelSelection::new("task", LabelOperator::NotExists, Vec::<String>::new()).is_ok()
        );
    }
}
