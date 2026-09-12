use std::collections::BTreeSet;

use crate::DomainError;

/// Upper bound on how many refs one named expansion may ask for. A named
/// expansion is a batch, not a way to re-request the whole selection one call
/// at a time and not a way to widen it.
pub const MAX_EXPANSION_REFS: usize = 64;

/// How a trace read wants canonical bodies delivered.
///
/// Every field is optional and every field is off by default. With all of them
/// absent this is the legacy read: every selected body is loaded, no descriptor
/// is consulted, no manifest is computed, and the response is the one the
/// no-option oracle froze. None of these options changes discovery, ranking,
/// which paths qualify or which refs are selected — only body delivery.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceBodyOptions {
    /// Ceiling on the stored record bytes this response may admit. Absent
    /// means unbounded, which is only legacy behaviour when no other option
    /// is active.
    pub max_record_bytes: Option<u64>,
    /// Load only these refs of the selected proof table. An empty set is a
    /// descriptor-only view: the manifest and every state, no canonical text.
    pub refs: Option<BTreeSet<String>>,
    /// The manifest this caller believes it is expanding. Checked before any
    /// body is read.
    pub expect_selection: Option<String>,
    /// Card language for compact presentation.
    pub compact: Option<String>,
}

impl TraceBodyOptions {
    /// Whether this read leaves the legacy body path.
    pub fn is_active(&self) -> bool {
        self.max_record_bytes.is_some()
            || self.refs.is_some()
            || self.expect_selection.is_some()
            || self.compact.is_some()
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if let Some(refs) = &self.refs {
            if refs.len() > MAX_EXPANSION_REFS {
                return Err(DomainError::InvalidState(format!(
                    "a named body expansion takes at most {MAX_EXPANSION_REFS} refs; \
                     ask for the rest in the next bound batch"
                )));
            }
            if refs.iter().any(|reference| reference.trim().is_empty()) {
                return Err(DomainError::InvalidState(
                    "a named body expansion takes nonempty refs".into(),
                ));
            }
            // A nonempty named expansion is a continuation of a selection
            // somebody already saw. Without the ceiling it is unbounded again,
            // and without the manifest it could join text from another one.
            if !refs.is_empty()
                && (self.max_record_bytes.is_none() || self.expect_selection.is_none())
            {
                return Err(DomainError::InvalidState(
                    "a named body expansion requires both a record byte ceiling and the \
                     expect_selection returned by the read that named these refs"
                        .into(),
                ));
            }
        }
        if self.max_record_bytes == Some(0) {
            return Err(DomainError::InvalidState(
                "a record byte ceiling is positive; omit it for the unbounded legacy read, \
                 or ask for an empty ref list to see descriptors only"
                    .into(),
            ));
        }
        if self.compact.as_ref().is_some_and(|l| l.trim().is_empty()) {
            return Err(DomainError::InvalidState(
                "compact presentation needs the language its cards were written in".into(),
            ));
        }
        Ok(())
    }
}
