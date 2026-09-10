use super::GuidanceError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentId(String);

impl AgentId {
    pub(crate) fn parse(value: &str) -> Result<Self, GuidanceError> {
        let valid = value.strip_prefix("agent_").is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if !valid {
            return Err(GuidanceError::InvalidSession(
                "copy the agent_id returned by KMP".into(),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
