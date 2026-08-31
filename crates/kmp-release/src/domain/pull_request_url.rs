use std::fmt::{Display, Formatter};

use crate::domain::release_error::ReleaseError;

/// One pull request, named the way GitHub names it.
///
/// A release chain hands this from the step that opens the pull request to
/// the step that arms auto-merge, and prints it as the one thing the operator
/// has to look at before the gate speaks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequestUrl(String);

impl PullRequestUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, ReleaseError> {
        let raw = raw.into();
        let value = raw.trim();
        if !value.starts_with("https://") || !value.contains("/pull/") {
            return Err(ReleaseError::invalid(format!(
                "`{value}` is not a pull request URL"
            )));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PullRequestUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_url_is_a_github_pull_request() {
        assert_eq!(
            PullRequestUrl::parse("  https://github.com/underpass-ai/kmp/pull/445\n")
                .expect("url")
                .as_str(),
            "https://github.com/underpass-ai/kmp/pull/445"
        );
    }

    #[test]
    fn anything_else_is_refused_rather_than_carried() {
        for value in [
            "",
            "445",
            "github.com/underpass-ai/kmp/pull/445",
            "https://github.com/underpass-ai/kmp/issues/445",
        ] {
            assert!(PullRequestUrl::parse(value).is_err(), "accepted {value}");
        }
    }
}
