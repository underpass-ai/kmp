use std::error::Error;
use std::fmt;

use crate::NatsConsumerError;

#[derive(Debug)]
pub enum NatsRuntimeError {
    Connection(String),
    StreamSetup(String),
    ConsumerSetup(String),
    Subscription(String),
    Message(String),
    Consumer(NatsConsumerError),
}

impl fmt::Display for NatsRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(message) => write!(f, "nats connection error: {message}"),
            Self::StreamSetup(message) => write!(f, "nats stream setup error: {message}"),
            Self::ConsumerSetup(message) => write!(f, "nats consumer setup error: {message}"),
            Self::Subscription(message) => write!(f, "nats subscription error: {message}"),
            Self::Message(message) => write!(f, "nats message error: {message}"),
            Self::Consumer(error) => error.fmt(f),
        }
    }
}

impl Error for NatsRuntimeError {}

#[cfg(test)]
mod tests {
    use crate::{NatsConsumerError, runtime::NatsRuntimeError};

    #[test]
    fn display_formats_each_runtime_error_variant() {
        assert_eq!(
            NatsRuntimeError::Connection("down".to_string()).to_string(),
            "nats connection error: down"
        );
        assert_eq!(
            NatsRuntimeError::StreamSetup("bad stream".to_string()).to_string(),
            "nats stream setup error: bad stream"
        );
        assert_eq!(
            NatsRuntimeError::ConsumerSetup("bad consumer".to_string()).to_string(),
            "nats consumer setup error: bad consumer"
        );
        assert_eq!(
            NatsRuntimeError::Subscription("closed".to_string()).to_string(),
            "nats subscription error: closed"
        );
        assert_eq!(
            NatsRuntimeError::Message("broken".to_string()).to_string(),
            "nats message error: broken"
        );
        assert_eq!(
            NatsRuntimeError::Consumer(NatsConsumerError::Publish("boom".to_string())).to_string(),
            "publish error: boom"
        );
    }
}
