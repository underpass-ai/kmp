use super::validation_error::WriteValidationError;
use crate::serving::ToolError;

/// Nonempty independent record failures; the rejected packet writes nothing.
#[derive(Debug)]
pub(crate) struct WriteValidationErrors(Vec<WriteValidationError>);

impl WriteValidationErrors {
    pub(crate) fn collected(mut errors: Vec<WriteValidationError>) -> Option<Self> {
        // A shared packet error can surface identically in adjacent members.
        errors.dedup();
        (!errors.is_empty()).then_some(Self(errors))
    }
}

impl From<WriteValidationError> for WriteValidationErrors {
    fn from(error: WriteValidationError) -> Self {
        Self(vec![error])
    }
}

impl From<String> for WriteValidationErrors {
    fn from(message: String) -> Self {
        WriteValidationError::from(message).into()
    }
}

impl From<&str> for WriteValidationErrors {
    fn from(message: &str) -> Self {
        WriteValidationError::from(message).into()
    }
}

impl WriteValidationErrors {
    /// Whether any failure is about labels, which only the store can say
    /// more about: the keys the about already uses.
    pub(crate) fn concern_labels(&self) -> bool {
        self.0.iter().any(|error| error.has_code(LABELS_CODE))
    }

    /// Names the about's own label keys in every labels failure, so the
    /// writer reuses its vocabulary instead of guessing one.
    pub(crate) fn name_label_keys(&mut self, keys: &[String]) {
        let sentence = if keys.is_empty() {
            " This about holds no labels yet; choose keys that describe the work.".to_owned()
        } else {
            format!(
                " This about already uses the label keys: {}.",
                keys.join(", ")
            )
        };
        for error in self
            .0
            .iter_mut()
            .filter(|error| error.has_code(LABELS_CODE))
        {
            error.append(&sentence);
        }
    }
}

/// The feedback code of every labels refusal.
const LABELS_CODE: &str = "INVALID_LABELS";

impl From<WriteValidationErrors> for ToolError {
    fn from(errors: WriteValidationErrors) -> Self {
        let count = errors.0.len();
        let records = errors
            .0
            .iter()
            .filter_map(WriteValidationError::record)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let mut errors = errors.0.into_iter();
        let mut result = Self::from(errors.next().expect("nonempty writer failures"));
        if count > 1 {
            let scope = match records {
                0 | 1 => String::new(),
                records => format!(" in {records} records"),
            };
            result.message = format!(
                "{count} validation failures{scope}; repair every listed field and resend the whole packet. No changes were written."
            );
            for error in errors {
                result.feedback.extend(Self::from(error).feedback);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(field: &str, code: &'static str) -> WriteValidationError {
        WriteValidationError::new(format!("{field} is wrong"))
            .at(field)
            .code(code)
    }

    #[test]
    fn several_failures_are_counted_by_record_and_all_travel_as_feedback() {
        let errors = WriteValidationErrors::collected(vec![
            failure("memories[0].labels", "INVALID_LABELS"),
            failure("memories[0].summary_en", "SEARCH_SUMMARY_REQUIRED"),
            failure("memories[2].kind", "INVALID_KIND"),
        ])
        .expect("three failures");
        let error = ToolError::from(errors);

        assert_eq!(
            error.message,
            "3 validation failures in 2 records; repair every listed field and resend the whole packet. No changes were written."
        );
        assert_eq!(error.feedback.len(), 3);
        assert_eq!(error.feedback[1]["field"], "memories[0].summary_en");
    }

    #[test]
    fn one_failure_keeps_its_own_reason() {
        let errors = WriteValidationErrors::from(failure("memories[0].kind", "INVALID_KIND"));
        let error = ToolError::from(errors);

        assert_eq!(error.message, "memories[0].kind is wrong");
        assert_eq!(error.feedback.len(), 1);
    }

    #[test]
    fn labels_failures_name_the_keys_the_about_uses() {
        let mut errors = WriteValidationErrors::collected(vec![
            failure("memories[0].labels", "INVALID_LABELS"),
            failure("memories[0].kind", "INVALID_KIND"),
        ])
        .expect("two failures");
        assert!(errors.concern_labels());

        errors.name_label_keys(&["component".to_owned(), "topic".to_owned()]);
        let error = ToolError::from(errors);

        assert_eq!(
            error.feedback[0]["reason"],
            "memories[0].labels is wrong This about already uses the label keys: component, topic."
        );
        assert_eq!(error.feedback[1]["reason"], "memories[0].kind is wrong");

        let mut fresh = WriteValidationErrors::from(failure("labels", "INVALID_LABELS"));
        fresh.name_label_keys(&[]);
        assert!(
            ToolError::from(fresh)
                .message
                .ends_with("This about holds no labels yet; choose keys that describe the work.")
        );

        let unrelated = WriteValidationErrors::from(failure("memories[0].kind", "INVALID_KIND"));
        assert!(!unrelated.concern_labels());
    }
}
