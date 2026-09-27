/// Why a judgement came back without answers, told apart for whoever reads
/// the warning: a request refused as too large (by the provider, or by the
/// budget before anything was sent) is not an unreachable Jev, and saying
/// "unavailable" for it sends the reader to the wrong fix.
///
/// The judgement port carries failures as text, so a refusal is text with a
/// fixed prefix, written by [`JudgementFailure::refused`] and read back by
/// [`JudgementFailure::read`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JudgementFailure<'a> {
    /// The request was refused as too large; its reason.
    Refused(&'a str),
    /// Jev could not be reached or did not answer as asked.
    Unavailable(&'a str),
}

const REFUSED: &str = "refused: ";

impl<'a> JudgementFailure<'a> {
    /// The failure text of a request refused for `reason`.
    pub(crate) fn refused(reason: &str) -> String {
        format!("{REFUSED}{reason}")
    }

    pub(crate) fn read(error: &'a str) -> Self {
        match error.strip_prefix(REFUSED) {
            Some(reason) => Self::Refused(reason),
            None => Self::Unavailable(error),
        }
    }

    /// The review warning for `error`, naming what the call did instead.
    pub(crate) fn warning(error: &str, instead: &str) -> String {
        match JudgementFailure::read(error) {
            JudgementFailure::Refused(reason) => {
                format!("Jev refused the request as too large ({reason}); {instead}")
            }
            JudgementFailure::Unavailable(error) => {
                format!("Jev unavailable; {instead}: {error}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_reported_with_its_reason_and_anything_else_as_unavailable() {
        let refused = JudgementFailure::refused("TypeSafe HTTP 400 max_tokens_exceeded");
        assert_eq!(
            JudgementFailure::read(&refused),
            JudgementFailure::Refused("TypeSafe HTTP 400 max_tokens_exceeded")
        );
        let warning = JudgementFailure::warning(&refused, "using kernel pairs only");
        assert_eq!(
            warning,
            "Jev refused the request as too large (TypeSafe HTTP 400 max_tokens_exceeded); using kernel pairs only"
        );
        assert!(!warning.contains("unavailable"));
        assert_eq!(
            JudgementFailure::warning("TypeSafe timed out", "using kernel pairs only"),
            "Jev unavailable; using kernel pairs only: TypeSafe timed out"
        );
    }
}
