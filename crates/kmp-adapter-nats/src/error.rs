use std::error::Error;
use std::fmt;

use kmp_domain::PortError;

#[derive(Debug)]
pub enum NatsConsumerError {
    UnsupportedSubject(String),
    InvalidPayload(String),
    InvalidEnvelope(String),
    InvalidRequest(String),
    Publish(String),
    MessageDisposition(String),
    Application(PortError),
}

impl fmt::Display for NatsConsumerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSubject(subject) => write!(f, "unsupported subject: {subject}"),
            Self::InvalidPayload(message) => write!(f, "invalid payload: {message}"),
            Self::InvalidEnvelope(message) => write!(f, "invalid envelope: {message}"),
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::Publish(message) => write!(f, "publish error: {message}"),
            Self::MessageDisposition(message) => write!(f, "message disposition error: {message}"),
            Self::Application(error) => error.fmt(f),
        }
    }
}

impl Error for NatsConsumerError {}

#[cfg(test)]
mod tests {
    use kmp_domain::PortError;

    use super::NatsConsumerError;

    #[test]
    fn display_covers_new_variants() {
        assert_eq!(
            NatsConsumerError::InvalidEnvelope("bad envelope".to_string()).to_string(),
            "invalid envelope: bad envelope"
        );
        assert_eq!(
            NatsConsumerError::InvalidRequest("bad request".to_string()).to_string(),
            "invalid request: bad request"
        );
        assert_eq!(
            NatsConsumerError::Publish("boom".to_string()).to_string(),
            "publish error: boom"
        );
        assert_eq!(
            NatsConsumerError::MessageDisposition("ack failed".to_string()).to_string(),
            "message disposition error: ack failed"
        );
        assert_eq!(
            NatsConsumerError::Application(PortError::InvalidState("bad".to_string())).to_string(),
            "bad"
        );
    }
}
