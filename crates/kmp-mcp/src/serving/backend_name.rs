//! The backend a value of `KMP_MCP_BACKEND` names.

/// One name per backend, case-insensitively. The aliases `live` and
/// `fixtures` were retired at 1.0, and `fixture` (canned answers that look
/// real) exists only in debug and `eval-hooks` builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendName {
    Embedded,
    Grpc,
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    Fixture,
}

impl BackendName {
    pub(crate) fn named(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "embedded" => Some(Self::Embedded),
            "grpc" => Some(Self::Grpc),
            #[cfg(any(debug_assertions, feature = "eval-hooks"))]
            "fixture" => Some(Self::Fixture),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BackendName;

    #[test]
    fn one_name_per_backend_and_the_aliases_are_gone() {
        assert_eq!(
            BackendName::named(" Embedded "),
            Some(BackendName::Embedded)
        );
        assert_eq!(BackendName::named("GRPC"), Some(BackendName::Grpc));
        assert_eq!(BackendName::named("fixture"), Some(BackendName::Fixture));
        for retired in ["live", "fixtures", "embeded", ""] {
            assert_eq!(BackendName::named(retired), None, "{retired}");
        }
    }
}
