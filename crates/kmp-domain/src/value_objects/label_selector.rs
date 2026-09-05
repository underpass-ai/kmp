use std::collections::BTreeSet;
use std::fmt;

use crate::error::DomainError;
use crate::value_objects::entry_labels::EntryLabels;
use crate::value_objects::memory_dimension_identity::MemoryDimensionIdentity;

/// How a selector reads the labels an entry stands in.
///
/// `In` and `NotIn` compare the values under a key; `Exists` and
/// `NotExists` ask only whether the key is there. A key an entry does not
/// carry satisfies `NotIn`, as it does in Kubernetes: "not in these values"
/// includes "has no value at all".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    NotExists,
}

impl LabelSelectorOperator {
    /// The operator as a caller names it: `in`, `notin`, `exists`,
    /// `notexists`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::NotIn => "notin",
            Self::Exists => "exists",
            Self::NotExists => "notexists",
        }
    }

    fn takes_values(self) -> bool {
        matches!(self, Self::In | Self::NotIn)
    }
}

impl fmt::Display for LabelSelectorOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A predicate over the labels one entry stands in, read as a map from key
/// to values: `env in (prod, staging)`, `customer notin (acme)`, `incident`,
/// `!task`.
///
/// This is a different thing from `only` and `except`, which look at one
/// coordinate at a time and keep an entry when *some* coordinate passes. A
/// selector looks at the whole entry, so `notexists task` keeps only the
/// entries with no `task` label at all, where `except task` would keep every
/// entry that also stands in a process. The two are not duals and are not
/// meant to be; a selector is the one to reach for when the question is
/// about what an entry is catalogued as.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LabelSelector {
    key: String,
    operator: LabelSelectorOperator,
    values: BTreeSet<String>,
}

impl LabelSelector {
    /// Builds a selector, refusing the shapes that mean nothing: an empty
    /// key, `in`/`notin` without values, `exists`/`notexists` with them. A
    /// value given as a namespaced scope id is read as its bare value, the
    /// way `scope_ids` reads either; the about is `scope`'s business.
    pub fn new(
        key: impl Into<String>,
        operator: LabelSelectorOperator,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DomainError> {
        let key = key.into().trim().to_string();
        if key.is_empty() {
            return Err(DomainError::EmptyValue("label selector key"));
        }
        let values = values
            .into_iter()
            .map(Into::into)
            .map(|value| bare_label_value(&value))
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        if operator.takes_values() && values.is_empty() {
            return Err(DomainError::InvalidState(format!(
                "label selector `{key} {operator}` requires at least one value"
            )));
        }
        if !operator.takes_values() && !values.is_empty() {
            return Err(DomainError::InvalidState(format!(
                "label selector `{key} {operator}` takes no values"
            )));
        }
        Ok(Self {
            key,
            operator,
            values,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn operator(&self) -> LabelSelectorOperator {
        self.operator
    }

    pub fn values(&self) -> &BTreeSet<String> {
        &self.values
    }

    /// Whether this selector can name the labels it wants, so an index can
    /// pick abouts by them: `exists` names a key, `in` names values. The
    /// negative forms exclude and cannot narrow anything.
    pub fn is_positive(&self) -> bool {
        matches!(
            self.operator,
            LabelSelectorOperator::In | LabelSelectorOperator::Exists
        )
    }

    /// Does an entry with these labels pass?
    pub fn admits(&self, labels: &EntryLabels) -> bool {
        match self.operator {
            LabelSelectorOperator::Exists => labels.has_key(&self.key),
            LabelSelectorOperator::NotExists => !labels.has_key(&self.key),
            LabelSelectorOperator::In => labels
                .values(&self.key)
                .is_some_and(|values| values.iter().any(|value| self.values.contains(value))),
            LabelSelectorOperator::NotIn => !labels
                .values(&self.key)
                .is_some_and(|values| values.iter().any(|value| self.values.contains(value))),
        }
    }
}

impl fmt::Display for LabelSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.operator {
            LabelSelectorOperator::Exists => formatter.write_str(&self.key),
            LabelSelectorOperator::NotExists => write!(formatter, "!{}", self.key),
            LabelSelectorOperator::In | LabelSelectorOperator::NotIn => {
                let values = self.values.iter().cloned().collect::<Vec<_>>().join(", ");
                write!(formatter, "{} {} ({values})", self.key, self.operator)
            }
        }
    }
}

/// A label value as the catalogue shows it: a namespaced scope id loses its
/// `about:…:dimension:` prefix, anything else is taken as written.
pub fn bare_label_value(value: &str) -> String {
    MemoryDimensionIdentity::parse(value)
        .map(|identity| identity.dimension_id().to_string())
        .unwrap_or_else(|| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> EntryLabels {
        EntryLabels::from_pairs(pairs.iter().map(|(key, value)| (*key, *value)))
    }

    #[test]
    fn in_and_notin_read_the_values_under_a_key() {
        let selector = LabelSelector::new("env", LabelSelectorOperator::In, ["prod", "staging"])
            .expect("selector");
        assert!(selector.admits(&labels(&[("env", "prod"), ("task", "t-1")])));
        assert!(!selector.admits(&labels(&[("env", "dev")])));
        assert!(!selector.admits(&labels(&[("task", "t-1")])));

        let selector =
            LabelSelector::new("env", LabelSelectorOperator::NotIn, ["prod"]).expect("selector");
        assert!(selector.admits(&labels(&[("env", "dev")])));
        assert!(!selector.admits(&labels(&[("env", "prod")])));
    }

    #[test]
    fn an_absent_key_satisfies_notin_as_kubernetes_reads_it() {
        let selector =
            LabelSelector::new("env", LabelSelectorOperator::NotIn, ["prod"]).expect("selector");
        assert!(selector.admits(&labels(&[("task", "t-1")])));
    }

    #[test]
    fn exists_and_notexists_ask_only_for_the_key() {
        let exists = LabelSelector::new(
            "incident",
            LabelSelectorOperator::Exists,
            Vec::<String>::new(),
        )
        .expect("selector");
        assert!(exists.admits(&labels(&[("incident", "north-outage")])));
        assert!(!exists.admits(&labels(&[("task", "t-1")])));

        let absent = LabelSelector::new(
            "task",
            LabelSelectorOperator::NotExists,
            Vec::<String>::new(),
        )
        .expect("selector");
        assert!(absent.admits(&labels(&[("agentic_process", "p-1")])));
        assert!(!absent.admits(&labels(&[("agentic_process", "p-1"), ("task", "t-1")])));
    }

    #[test]
    fn a_namespaced_value_is_read_as_its_bare_value() {
        let selector = LabelSelector::new(
            "incident",
            LabelSelectorOperator::In,
            ["about:project:kmp:dimension:north-outage"],
        )
        .expect("selector");
        assert_eq!(
            selector.values().iter().cloned().collect::<Vec<_>>(),
            vec!["north-outage".to_string()]
        );
        assert!(selector.admits(&labels(&[("incident", "north-outage")])));
    }

    #[test]
    fn the_shapes_that_mean_nothing_are_refused() {
        assert!(
            LabelSelector::new(" ", LabelSelectorOperator::Exists, Vec::<String>::new()).is_err()
        );
        assert!(
            LabelSelector::new("env", LabelSelectorOperator::In, Vec::<String>::new()).is_err()
        );
        assert!(LabelSelector::new("env", LabelSelectorOperator::In, [" "]).is_err());
        assert!(LabelSelector::new("env", LabelSelectorOperator::Exists, ["prod"]).is_err());
    }

    #[test]
    fn a_selector_reads_back_the_way_a_caller_would_write_it() {
        assert_eq!(
            LabelSelector::new("env", LabelSelectorOperator::In, ["staging", "prod"])
                .expect("selector")
                .to_string(),
            "env in (prod, staging)"
        );
        assert_eq!(
            LabelSelector::new(
                "task",
                LabelSelectorOperator::NotExists,
                Vec::<String>::new()
            )
            .expect("selector")
            .to_string(),
            "!task"
        );
    }
}
