//! The keyed fingerprints one call leaves in its log line.

/// A call's keyed fingerprints: what it asked (a question or a wake
/// intent) and the guidance context it ran in.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct CallFingerprints {
    pub(crate) subject: Option<String>,
    pub(crate) context: Option<String>,
}
