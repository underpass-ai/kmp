use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::Duration;

use url::Url;

pub const HTTP_ADDR_ENV: &str = "KMP_MCP_HTTP_ADDR";
pub const PUBLIC_URL_ENV: &str = "KMP_MCP_HTTP_PUBLIC_URL";
pub const AUTH_ISSUER_ENV: &str = "KMP_MCP_HTTP_AUTH_ISSUER";
pub const AUTH_AUDIENCE_ENV: &str = "KMP_MCP_HTTP_AUTH_AUDIENCE";
pub const AUTH_JWKS_URI_ENV: &str = "KMP_MCP_HTTP_AUTH_JWKS_URI";
pub const ALLOWED_ORIGINS_ENV: &str = "KMP_MCP_HTTP_ALLOWED_ORIGINS";
pub const REQUEST_TIMEOUT_SECS_ENV: &str = "KMP_MCP_HTTP_REQUEST_TIMEOUT_SECS";
pub const MAX_BODY_BYTES_ENV: &str = "KMP_MCP_HTTP_MAX_BODY_BYTES";
pub const REQUIRE_GRPC_MTLS_ENV: &str = "KMP_MCP_HTTP_REQUIRE_GRPC_MTLS";

const DEFAULT_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_TIMEOUT_SECS: u64 = 20;
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;

#[derive(Clone, Debug)]
pub struct HttpGatewayConfig {
    pub bind_addr: SocketAddr,
    pub public_url: Url,
    pub issuer: Url,
    pub audience: String,
    pub jwks_uri: Option<Url>,
    pub allowed_origins: BTreeSet<String>,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    pub require_grpc_mtls: bool,
}

impl HttpGatewayConfig {
    pub fn from_env() -> Result<Self, String> {
        let bind_addr = env_or(HTTP_ADDR_ENV, DEFAULT_ADDR)
            .parse::<SocketAddr>()
            .map_err(|error| format!("{HTTP_ADDR_ENV} must be a socket address: {error}"))?;
        let public_url = required_url(PUBLIC_URL_ENV)?;
        validate_public_url(&public_url)?;
        let issuer = required_url(AUTH_ISSUER_ENV)?;
        require_https(AUTH_ISSUER_ENV, &issuer)?;
        if issuer.query().is_some() || issuer.fragment().is_some() {
            return Err(format!(
                "{AUTH_ISSUER_ENV} must not contain a query or fragment"
            ));
        }
        let audience = required_env(AUTH_AUDIENCE_ENV)?;
        let jwks_uri = optional_env(AUTH_JWKS_URI_ENV)
            .map(|value| parse_url(AUTH_JWKS_URI_ENV, &value))
            .transpose()?;
        if let Some(uri) = jwks_uri.as_ref() {
            require_https(AUTH_JWKS_URI_ENV, uri)?;
        }
        let mut allowed_origins = optional_env(ALLOWED_ORIGINS_ENV)
            .map(|value| parse_origins(&value))
            .transpose()?
            .unwrap_or_default();
        allowed_origins.insert(public_url.origin().ascii_serialization());
        let timeout_secs =
            optional_positive_u64(REQUEST_TIMEOUT_SECS_ENV)?.unwrap_or(DEFAULT_TIMEOUT_SECS);
        if timeout_secs > 300 {
            return Err(format!("{REQUEST_TIMEOUT_SECS_ENV} must be at most 300"));
        }
        let max_body_bytes =
            optional_positive_usize(MAX_BODY_BYTES_ENV)?.unwrap_or(DEFAULT_MAX_BODY_BYTES);
        if !(1_024..=10 * 1_048_576).contains(&max_body_bytes) {
            return Err(format!(
                "{MAX_BODY_BYTES_ENV} must be between 1024 and 10485760"
            ));
        }
        let require_grpc_mtls = optional_env(REQUIRE_GRPC_MTLS_ENV)
            .map(|value| parse_bool(REQUIRE_GRPC_MTLS_ENV, &value))
            .transpose()?
            .unwrap_or(true);

        Ok(Self {
            bind_addr,
            public_url,
            issuer,
            audience,
            jwks_uri,
            allowed_origins,
            request_timeout: Duration::from_secs(timeout_secs),
            max_body_bytes,
            require_grpc_mtls,
        })
    }

    pub fn resource_metadata_url(&self) -> String {
        let mut metadata = self.public_url.clone();
        metadata.set_path("/.well-known/oauth-protected-resource");
        metadata.set_query(None);
        metadata.set_fragment(None);
        metadata.to_string()
    }
}

fn validate_public_url(url: &Url) -> Result<(), String> {
    require_https(PUBLIC_URL_ENV, url)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(format!(
            "{PUBLIC_URL_ENV} must not contain a query or fragment"
        ));
    }
    if url.path().trim_end_matches('/') != "/mcp" {
        return Err(format!("{PUBLIC_URL_ENV} must identify the /mcp endpoint"));
    }
    Ok(())
}

fn require_https(name: &str, url: &Url) -> Result<(), String> {
    if url.scheme() != "https" {
        return Err(format!("{name} must use https"));
    }
    Ok(())
}

fn required_url(name: &str) -> Result<Url, String> {
    parse_url(name, &required_env(name)?)
}

fn parse_url(name: &str, value: &str) -> Result<Url, String> {
    Url::parse(value).map_err(|error| format!("{name} must be an absolute URL: {error}"))
}

fn required_env(name: &str) -> Result<String, String> {
    optional_env(name).ok_or_else(|| format!("{name} is required"))
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_or(name: &str, fallback: &str) -> String {
    optional_env(name).unwrap_or_else(|| fallback.to_string())
}

fn parse_origins(value: &str) -> Result<BTreeSet<String>, String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let url = Url::parse(value).map_err(|error| {
                format!("{ALLOWED_ORIGINS_ENV} contains an invalid URL: {error}")
            })?;
            if !matches!(url.scheme(), "http" | "https")
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(format!(
                    "{ALLOWED_ORIGINS_ENV} values must be HTTP origins without paths"
                ));
            }
            Ok(url.origin().ascii_serialization())
        })
        .collect()
}

fn optional_positive_u64(name: &str) -> Result<Option<u64>, String> {
    optional_env(name)
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{name} must be a positive integer"))
        })
        .transpose()
}

fn optional_positive_usize(name: &str) -> Result<Option<usize>, String> {
    optional_env(name)
        .map(|value| {
            value
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{name} must be a positive integer"))
        })
        .transpose()
}

fn parse_bool(name: &str, value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("{name} must be true or false")),
    }
}
