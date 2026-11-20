use super::ContinuationError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadContinuationId(String);

impl ReadContinuationId {
    pub(crate) fn parse(value: &str) -> Result<Self, ContinuationError> {
        if !value.strip_prefix("read_").is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(ContinuationError::Invalid(
                "continuation must be the exact read identifier returned by KMP".into(),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// A fresh 128-bit handle. It names a call; it is not a credential.
    pub(crate) fn random() -> Result<Self, ContinuationError> {
        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            ContinuationError::Unavailable(format!("continuation entropy: {error}"))
        })?;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Self::parse(&format!("read_{hex}"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
