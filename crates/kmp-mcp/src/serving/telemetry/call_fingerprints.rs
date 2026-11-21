//! The keyed fingerprints one call leaves in its log line.

/// A call's keyed fingerprint of what it asked: a question or a wake intent.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct CallFingerprints {
    pub(crate) subject: Option<String>,
}
