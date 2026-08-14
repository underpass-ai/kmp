use kmp_ports::PortError;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_PORT: u16 = 6379;
pub(crate) const DEFAULT_KEY_PREFIX: &str = "rehydration:snapshot";
pub(crate) const DEFAULT_NODE_DETAIL_KEY_PREFIX: &str = "rehydration:node-detail";
pub(crate) const DEFAULT_PROCESSED_EVENT_KEY_PREFIX: &str = "rehydration:processed-event";
pub(crate) const DEFAULT_PROJECTION_CHECKPOINT_KEY_PREFIX: &str =
    "rehydration:projection-checkpoint";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValkeyEndpoint {
    pub raw_uri: String,
    pub host: String,
    pub port: u16,
    pub key_prefix: String,
    pub ttl_seconds: Option<u64>,
    pub tls: ValkeyTlsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ValkeyTlsConfig {
    pub enabled: bool,
    pub ca_path: Option<PathBuf>,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
}

impl ValkeyEndpoint {
    pub(crate) fn parse(snapshot_uri: String) -> Result<Self, PortError> {
        Self::parse_with_default_key_prefix(snapshot_uri, "snapshot", DEFAULT_KEY_PREFIX)
    }

    pub(crate) fn parse_with_default_key_prefix(
        raw_uri: String,
        name: &str,
        default_key_prefix: &str,
    ) -> Result<Self, PortError> {
        if raw_uri.trim().is_empty() {
            return Err(PortError::InvalidState(format!(
                "{name} uri cannot be empty"
            )));
        }

        let (scheme, authority, query) = split_uri(&raw_uri, name)?;
        let tls_enabled = matches!(scheme, "rediss" | "valkeys");
        if !matches!(scheme, "redis" | "valkey" | "rediss" | "valkeys") {
            return Err(PortError::InvalidState(format!(
                "unsupported {name} scheme `{scheme}`"
            )));
        }

        let (host, port) = parse_authority(authority, DEFAULT_PORT, name)?;

        let mut key_prefix = default_key_prefix.to_string();
        let mut ttl_seconds = None;
        let mut tls = ValkeyTlsConfig {
            enabled: tls_enabled,
            ..ValkeyTlsConfig::default()
        };
        if let Some(query) = query {
            for pair in query.split('&') {
                if pair.is_empty() {
                    continue;
                }

                let (key, value) = pair.split_once('=').ok_or_else(|| {
                    PortError::InvalidState(format!(
                        "{name} uri query parameter `{pair}` is invalid"
                    ))
                })?;

                match key {
                    "key_prefix" => {
                        if value.trim().is_empty() {
                            return Err(PortError::InvalidState(format!(
                                "{name} key_prefix cannot be empty"
                            )));
                        }
                        key_prefix = value.to_string();
                    }
                    "ttl_seconds" => {
                        let ttl = value.parse::<u64>().map_err(|error| {
                            PortError::InvalidState(format!(
                                "{name} ttl_seconds must be an integer: {error}"
                            ))
                        })?;
                        ttl_seconds = Some(ttl);
                    }
                    "tls_ca_path" => tls.ca_path = Some(parse_tls_path(value, name, key)?),
                    "tls_cert_path" => tls.cert_path = Some(parse_tls_path(value, name, key)?),
                    "tls_key_path" => tls.key_path = Some(parse_tls_path(value, name, key)?),
                    _ => {
                        return Err(PortError::InvalidState(format!(
                            "unsupported {name} uri option `{key}`"
                        )));
                    }
                }
            }
        }

        validate_tls_options(name, &tls)?;

        Ok(Self {
            raw_uri,
            host,
            port,
            key_prefix,
            ttl_seconds,
            tls,
        })
    }

    pub(crate) fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub(crate) fn server_name(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&self.host)
    }
}

pub(crate) fn split_uri<'a>(
    raw_uri: &'a str,
    name: &str,
) -> Result<(&'a str, &'a str, Option<&'a str>), PortError> {
    let (scheme, remainder) = raw_uri
        .split_once("://")
        .ok_or_else(|| PortError::InvalidState(format!("{name} uri must include a scheme")))?;
    if scheme.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} uri must include a scheme"
        )));
    }

    let (location, query) = match remainder.split_once('?') {
        Some((location, query)) => (location, Some(query)),
        None => (remainder, None),
    };
    let authority = location.split('/').next().unwrap_or_default().trim();
    if authority.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} uri must include a host"
        )));
    }

    Ok((scheme, authority, query))
}

pub(crate) fn parse_authority(
    authority: &str,
    default_port: u16,
    name: &str,
) -> Result<(String, u16), PortError> {
    if authority.contains('@') {
        return Err(PortError::InvalidState(format!(
            "{name} uri auth segments are not supported yet"
        )));
    }

    if authority.starts_with('[') {
        let (host, remainder) = authority.split_once(']').ok_or_else(|| {
            PortError::InvalidState(format!("{name} uri contains an invalid IPv6 host"))
        })?;
        let host = format!("{host}]");
        let port = parse_optional_port(remainder, default_port, name)?;
        return Ok((host, port));
    }

    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            if host.is_empty() {
                return Err(PortError::InvalidState(format!(
                    "{name} uri must include a host"
                )));
            }
            let port = port.parse::<u16>().map_err(|error| {
                PortError::InvalidState(format!("{name} uri contains an invalid port: {error}"))
            })?;
            Ok((host.to_string(), port))
        }
        Some(_) => Err(PortError::InvalidState(format!(
            "{name} uri IPv6 hosts must use bracket notation"
        ))),
        None => Ok((authority.to_string(), default_port)),
    }
}

pub(crate) fn parse_optional_port(
    remainder: &str,
    default_port: u16,
    name: &str,
) -> Result<u16, PortError> {
    if remainder.is_empty() {
        return Ok(default_port);
    }
    let port = remainder.strip_prefix(':').ok_or_else(|| {
        PortError::InvalidState(format!("{name} uri contains an invalid port separator"))
    })?;
    port.parse::<u16>().map_err(|error| {
        PortError::InvalidState(format!("{name} uri contains an invalid port: {error}"))
    })
}

fn parse_tls_path(value: &str, name: &str, key: &str) -> Result<PathBuf, PortError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} {key} cannot be empty"
        )));
    }

    Ok(Path::new(trimmed).to_path_buf())
}

fn validate_tls_options(name: &str, tls: &ValkeyTlsConfig) -> Result<(), PortError> {
    if !tls.enabled && (tls.ca_path.is_some() || tls.cert_path.is_some() || tls.key_path.is_some())
    {
        return Err(PortError::InvalidState(format!(
            "{name} TLS options require rediss:// or valkeys://"
        )));
    }

    match (&tls.cert_path, &tls.key_path) {
        (Some(_), Some(_)) | (None, None) => Ok(()),
        _ => Err(PortError::InvalidState(format!(
            "{name} tls_cert_path and tls_key_path must be configured together"
        ))),
    }
}
