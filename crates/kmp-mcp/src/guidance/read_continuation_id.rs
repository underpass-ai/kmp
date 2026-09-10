use super::GuidanceError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadContinuationId(String);

impl ReadContinuationId {
    pub(crate) fn parse(value: &str) -> Result<Self, GuidanceError> {
        if !value.strip_prefix("read_").is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(GuidanceError::InvalidSession(
                "continuation must be the exact read identifier returned by KMP".into(),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
