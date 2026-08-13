mod consumer;
pub mod context_event_store;
mod error;
mod payload_decoding;
mod runtime;
mod subject_routing;

#[cfg(test)]
mod tests;

pub use consumer::NatsProjectionConsumer;
pub use context_event_store::NatsContextEventStore;
pub use error::NatsConsumerError;
pub use runtime::{
    NatsClientTlsConfig, NatsProjectionRuntime, NatsRuntimeError, connect_nats_client,
};
