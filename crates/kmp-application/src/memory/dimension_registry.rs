use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::MemoryDimensionIdentity;

use crate::ApplicationError;

use super::ExistingMemoryRefs;

/// Resolves logical labels to their stored refs, refusing
/// ambiguous bare references. Coordinates have a key; arbitrary
/// relation targets do not, so only the latter can be ambiguous.
pub(super) struct DimensionRegistry {
    about: String,
    by_label: BTreeMap<(String, String), String>,
    by_ref: BTreeSet<String>,
    aliases: BTreeMap<String, BTreeSet<String>>,
}

impl DimensionRegistry {
    pub(super) fn new(
        about: &str,
        existing: &ExistingMemoryRefs,
    ) -> Result<Self, ApplicationError> {
        let mut registry = Self {
            about: about.to_string(),
            by_label: BTreeMap::new(),
            by_ref: BTreeSet::new(),
            aliases: BTreeMap::new(),
        };
        for reference in &existing.dimensions {
            let identity = MemoryDimensionIdentity::resolve(about, reference).ok_or_else(|| {
                ApplicationError::Validation(format!("unsupported or foreign dimension ref `{reference}`; expected a label:v1 ref in `{about}`"))
            })?;
            registry.remember(identity.key(), identity.dimension_id(), reference);
        }
        Ok(registry)
    }

    pub(super) fn declare(&mut self, key: &str, value: &str) -> Result<String, ApplicationError> {
        let bare = self.value(key, value)?;
        let reference = self
            .by_label
            .get(&(key.to_string(), bare.clone()))
            .cloned()
            .unwrap_or(
                MemoryDimensionIdentity::new(&self.about, key, &bare)
                    .map_err(|error| ApplicationError::Validation(error.to_string()))?
                    .node_id(),
            );
        self.remember(key, &bare, &reference);
        self.aliases
            .entry(value.to_string())
            .or_default()
            .insert(reference.clone());
        Ok(reference)
    }

    pub(super) fn value(&self, key: &str, value: &str) -> Result<String, ApplicationError> {
        let Some(identity) = MemoryDimensionIdentity::parse(value) else {
            return Ok(value.trim().to_string());
        };
        if identity.about() != self.about {
            return Err(ApplicationError::Validation(format!(
                "memory dimension `{value}` belongs to another about; expected `{}`",
                self.about
            )));
        }
        let stored_key = identity.key();
        if stored_key != key {
            return Err(ApplicationError::Validation(format!(
                "memory dimension ref `{value}` names key `{stored_key}`, not `{key}`; use the bare value to declare a different label"
            )));
        }
        Ok(identity.dimension_id().to_string())
    }

    pub(super) fn coordinate(&self, key: &str, value: &str) -> Result<String, ApplicationError> {
        let bare = self.value(key, value)?;
        self.by_label
            .get(&(key.to_string(), bare))
            .cloned()
            .ok_or_else(|| {
                ApplicationError::Validation(format!(
                    "memory coordinate references unknown dimension `{key}={value}`"
                ))
            })
    }

    pub(super) fn member(&self, value: &str) -> Result<String, ApplicationError> {
        if self.by_ref.contains(value)
            || value == self.about
            || value.starts_with(&format!("{}:", self.about))
            || value.starts_with(&format!("evidence:{}:", self.about))
        {
            return Ok(value.to_string());
        }
        match self.aliases.get(value) {
            Some(refs) if refs.len() > 1 => Err(ApplicationError::Validation(format!(
                "memory dimension `{value}` is ambiguous across keys; use an exact dimension ref: {}",
                refs.iter().cloned().collect::<Vec<_>>().join(", ")
            ))),
            Some(refs) => Ok(refs.first().cloned().unwrap_or_else(|| value.to_string())),
            None => Ok(value.to_string()),
        }
    }

    fn remember(&mut self, key: &str, value: &str, reference: &str) {
        self.by_label
            .insert((key.to_string(), value.to_string()), reference.to_string());
        self.by_ref.insert(reference.to_string());
        self.aliases
            .entry(value.to_string())
            .or_default()
            .insert(reference.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_existing_membership_when_a_second_key_reuses_its_value() {
        let old = "label:v1:project%3Ax:alias:neb";
        let existing = ExistingMemoryRefs {
            dimensions: BTreeSet::from([old.to_string()]),
            ..Default::default()
        };
        let mut registry = DimensionRegistry::new("project:x", &existing).expect("registry");
        assert_eq!(registry.declare("alias", "neb").expect("reuse"), old);
        let component = registry.declare("component", "neb").expect("second key");
        assert_ne!(component, old);
        assert_eq!(registry.coordinate("alias", "neb").expect("alias"), old);
        assert_eq!(
            registry.coordinate("component", "neb").expect("component"),
            component
        );
        assert!(registry.member("neb").is_err());
        assert_eq!(registry.member(old).expect("exact old ref"), old);
        assert!(registry.coordinate("component", old).is_err());
    }

    #[test]
    fn labels_with_different_keys_and_multiple_values_resolve_independently() {
        let mut registry =
            DimensionRegistry::new("project:x", &ExistingMemoryRefs::default()).expect("registry");
        let alias = registry.declare("alias", "neb").expect("alias");
        let full = registry
            .declare("alias", "Nébula Cache")
            .expect("full alias");
        let component = registry.declare("component", "neb").expect("component");
        assert_ne!(alias, component);
        assert_ne!(alias, full);
        assert_eq!(
            registry
                .coordinate("alias", "Nébula Cache")
                .expect("lookup"),
            full
        );
        assert_eq!(registry.declare("alias", "neb").expect("reuse"), alias);
        let foreign = MemoryDimensionIdentity::new("project:y", "alias", "neb")
            .expect("foreign")
            .node_id();
        assert!(registry.declare("alias", &foreign).is_err());
    }
}
