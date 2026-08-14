use std::env;
use std::io;

use crate::{NatsTlsConfig, nats_tls_config::NatsEndpointConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRuntimeConfig {
    pub nats_url: String,
    pub runtime_state_uri: String,
    pub nats_tls: NatsTlsConfig,
}

impl ProjectionRuntimeConfig {
    pub fn from_env() -> Self {
        Self::try_from_env().expect("projection runtime config should be valid")
    }

    pub fn try_from_env() -> io::Result<Self> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    fn from_lookup<F>(lookup: F) -> io::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let endpoint = NatsEndpointConfig::from_lookup(&lookup)?;

        Ok(Self {
            nats_url: endpoint.url,
            runtime_state_uri: lookup("KMP_RUNTIME_STATE_URI")
                .unwrap_or_else(|| "redis://localhost:6379".to_string()),
            nats_tls: endpoint.tls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectionRuntimeConfig;

    #[test]
    fn defaults_match_generic_projection_runtime_contract() {
        let config = ProjectionRuntimeConfig::from_env();

        assert_eq!(config.nats_url, "nats://nats:4222");
        assert_eq!(config.runtime_state_uri, "redis://localhost:6379");
        assert_eq!(config.nats_tls, crate::NatsTlsConfig::disabled());
    }
}
