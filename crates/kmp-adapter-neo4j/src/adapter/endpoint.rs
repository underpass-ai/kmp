use std::path::{Path, PathBuf};

use kmp_ports::PortError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Neo4jEndpoint {
    pub(crate) connection_uri: String,
    pub(crate) user: String,
    pub(crate) password: String,
    pub(crate) tls_ca_path: Option<PathBuf>,
    pub(crate) tls_cert_path: Option<PathBuf>,
    pub(crate) tls_key_path: Option<PathBuf>,
}

pub(crate) struct UriParts<'a> {
    pub(crate) scheme: &'a str,
    pub(crate) authority: &'a str,
    pub(crate) query: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorityParts {
    pub(crate) host_port: String,
    pub(crate) user: Option<String>,
    pub(crate) password: Option<String>,
}

impl Neo4jEndpoint {
    pub(crate) fn parse(graph_uri: String) -> Result<Self, PortError> {
        let graph_uri = graph_uri.trim().to_string();
        if graph_uri.is_empty() {
            return Err(PortError::InvalidState(
                "graph uri cannot be empty".to_string(),
            ));
        }

        let uri = split_uri(&graph_uri, "graph")?;
        if !matches!(
            uri.scheme,
            "neo4j" | "neo4j+s" | "neo4j+ssc" | "bolt" | "bolt+s" | "bolt+ssc"
        ) {
            return Err(PortError::InvalidState(format!(
                "unsupported graph scheme `{}`",
                uri.scheme
            )));
        }

        let authority = parse_authority(uri.authority, "graph")?;
        let tls_enabled = matches!(uri.scheme, "neo4j+s" | "neo4j+ssc" | "bolt+s" | "bolt+ssc");
        let tls_paths = parse_query_params(uri.query, "graph", tls_enabled)?;

        Ok(Self {
            connection_uri: format!("{}://{}", uri.scheme, authority.host_port),
            user: authority.user.unwrap_or_default(),
            password: authority.password.unwrap_or_default(),
            tls_ca_path: tls_paths.ca,
            tls_cert_path: tls_paths.cert,
            tls_key_path: tls_paths.key,
        })
    }
}

struct TlsPaths {
    ca: Option<PathBuf>,
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
}

fn parse_query_params(
    query: Option<&str>,
    name: &str,
    tls_enabled: bool,
) -> Result<TlsPaths, PortError> {
    let Some(query) = query else {
        return Ok(TlsPaths {
            ca: None,
            cert: None,
            key: None,
        });
    };

    let mut ca = None;
    let mut cert = None;
    let mut key = None;

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }

        let (k, value) = pair.split_once('=').ok_or_else(|| {
            PortError::InvalidState(format!("{name} uri query parameter `{pair}` is invalid"))
        })?;

        match k {
            "tls_ca_path" | "tls_cert_path" | "tls_key_path" => {
                if !tls_enabled {
                    return Err(PortError::InvalidState(format!(
                        "graph {k} requires bolt+s, bolt+ssc, neo4j+s, or neo4j+ssc"
                    )));
                }
                let path = parse_tls_path(value, name, k)?;
                match k {
                    "tls_ca_path" => ca = Some(path),
                    "tls_cert_path" => cert = Some(path),
                    "tls_key_path" => key = Some(path),
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(PortError::InvalidState(format!(
                    "unsupported graph uri option `{k}`"
                )));
            }
        }
    }

    if cert.is_some() != key.is_some() {
        return Err(PortError::InvalidState(
            "graph tls_cert_path and tls_key_path must be configured together".to_string(),
        ));
    }

    Ok(TlsPaths { ca, cert, key })
}

pub(crate) fn split_uri<'a>(raw_uri: &'a str, name: &str) -> Result<UriParts<'a>, PortError> {
    let (scheme, remainder) = raw_uri
        .split_once("://")
        .ok_or_else(|| PortError::InvalidState(format!("{name} uri must include a scheme")))?;
    if scheme.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} uri must include a scheme"
        )));
    }

    let (before_query, query) = match remainder.split_once('?') {
        Some((authority_and_path, query)) => (authority_and_path, Some(query)),
        None => (remainder, None),
    };

    let (authority, path) = match before_query.split_once('/') {
        Some((authority, path)) => (authority.trim(), path),
        None => (before_query.trim(), ""),
    };
    if authority.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} uri must include a host"
        )));
    }
    if !path.is_empty() {
        return Err(PortError::InvalidState(format!(
            "{name} uri path segments are not supported"
        )));
    }

    Ok(UriParts {
        scheme,
        authority,
        query,
    })
}

pub(crate) fn parse_authority(authority: &str, name: &str) -> Result<AuthorityParts, PortError> {
    let (credentials, host_port) = match authority.rsplit_once('@') {
        Some((credentials, host_port)) => (Some(credentials), host_port),
        None => (None, authority),
    };

    let (user, password) = match credentials {
        Some(credentials) => {
            let (user, password) = credentials.split_once(':').ok_or_else(|| {
                PortError::InvalidState(format!(
                    "{name} uri auth segments must include username and password"
                ))
            })?;
            if user.is_empty() || password.is_empty() {
                return Err(PortError::InvalidState(format!(
                    "{name} uri auth segments must include username and password"
                )));
            }
            (Some(user.to_string()), Some(password.to_string()))
        }
        None => (None, None),
    };

    parse_host_port(host_port, name)?;

    Ok(AuthorityParts {
        host_port: host_port.to_string(),
        user,
        password,
    })
}

pub(crate) fn parse_host_port(authority: &str, name: &str) -> Result<(), PortError> {
    if authority.starts_with('[') {
        let (_, remainder) = authority.split_once(']').ok_or_else(|| {
            PortError::InvalidState(format!("{name} uri contains an invalid IPv6 host"))
        })?;
        if !remainder.is_empty() && !remainder.starts_with(':') {
            return Err(PortError::InvalidState(format!(
                "{name} uri contains an invalid port separator"
            )));
        }
        if let Some(port) = remainder.strip_prefix(':') {
            port.parse::<u16>().map_err(|error| {
                PortError::InvalidState(format!("{name} uri contains an invalid port: {error}"))
            })?;
        }
        return Ok(());
    }

    if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() {
            return Err(PortError::InvalidState(format!(
                "{name} uri must include a host"
            )));
        }
        if !port.is_empty() {
            port.parse::<u16>().map_err(|error| {
                PortError::InvalidState(format!("{name} uri contains an invalid port: {error}"))
            })?;
        }
    }

    Ok(())
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
