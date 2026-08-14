use kmp_adapter_nats::NatsClientTlsConfig;
use kmp_config::{NatsTlsConfig, NatsTlsMode};

pub(crate) fn adapter_nats_tls_config(config: &NatsTlsConfig) -> NatsClientTlsConfig {
    NatsClientTlsConfig {
        require_tls: config.mode != NatsTlsMode::Disabled,
        ca_path: config.ca_path.clone(),
        cert_path: config.cert_path.clone(),
        key_path: config.key_path.clone(),
        tls_first: config.tls_first,
    }
}
