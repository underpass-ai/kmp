use std::time::Duration;

use jsonwebtoken::jwk::JwkSet;
use serde::Deserialize;
use url::Url;

use crate::application::dto::discovery_document_dto::DiscoveryDocumentDto;
use crate::domain::auth_error::AuthError;

const MAX_DISCOVERY_BYTES: usize = 256 * 1024;
const MAX_JWKS_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct OidcDiscoveryClient {
    client: reqwest::Client,
}

impl OidcDiscoveryClient {
    pub fn new() -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| AuthError::Unavailable(format!("OIDC client: {error}")))?;
        Ok(Self { client })
    }

    pub async fn resolve_keys(
        &self,
        issuer: &Url,
        explicit_jwks_uri: Option<Url>,
    ) -> Result<(Url, JwkSet), AuthError> {
        let jwks_uri = match explicit_jwks_uri {
            Some(uri) => uri,
            None => self.discover_jwks_uri(issuer).await?,
        };
        if jwks_uri.scheme() != "https" {
            return Err(AuthError::Unavailable(
                "OIDC jwks_uri must use https".to_string(),
            ));
        }
        Self::require_issuer_origin(issuer, &jwks_uri)?;
        let keys = self.fetch_jwks(&jwks_uri).await?;
        Ok((jwks_uri, keys))
    }

    pub async fn fetch_jwks(&self, uri: &Url) -> Result<JwkSet, AuthError> {
        self.fetch_json(uri.clone(), MAX_JWKS_BYTES).await
    }

    async fn discover_jwks_uri(&self, issuer: &Url) -> Result<Url, AuthError> {
        let mut discovery_url = issuer.clone();
        let path = format!(
            "{}/.well-known/openid-configuration",
            issuer.path().trim_end_matches('/')
        );
        discovery_url.set_path(&path);
        discovery_url.set_query(None);
        discovery_url.set_fragment(None);
        let document: DiscoveryDocumentDto =
            self.fetch_json(discovery_url, MAX_DISCOVERY_BYTES).await?;
        if document.issuer != issuer.as_str() {
            return Err(AuthError::Unavailable(
                "OIDC discovery issuer does not match configured issuer".to_string(),
            ));
        }
        Url::parse(&document.jwks_uri)
            .map_err(|error| AuthError::Unavailable(format!("OIDC jwks_uri is invalid: {error}")))
    }

    fn require_issuer_origin(issuer: &Url, jwks_uri: &Url) -> Result<(), AuthError> {
        if jwks_uri.origin() != issuer.origin() {
            return Err(AuthError::Unavailable(
                "OIDC jwks_uri must share the configured issuer origin".to_string(),
            ));
        }
        Ok(())
    }

    async fn fetch_json<T: for<'de> Deserialize<'de>>(
        &self,
        uri: Url,
        max_bytes: usize,
    ) -> Result<T, AuthError> {
        let mut response = self
            .client
            .get(uri)
            .send()
            .await
            .map_err(|error| AuthError::Unavailable(format!("OIDC fetch failed: {error}")))?
            .error_for_status()
            .map_err(|error| AuthError::Unavailable(format!("OIDC endpoint failed: {error}")))?;
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(AuthError::Unavailable(
                "OIDC response exceeded the size limit".to_string(),
            ));
        }
        let mut bytes = Vec::with_capacity(max_bytes.min(16 * 1024));
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AuthError::Unavailable(format!("OIDC response failed: {error}")))?
        {
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(AuthError::Unavailable(
                    "OIDC response exceeded the size limit".to_string(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| AuthError::Unavailable(format!("OIDC response is invalid: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwks_uri_cannot_leave_the_operator_configured_issuer_origin() {
        let issuer = Url::parse("https://id.example/tenant").expect("issuer");
        for uri in [
            "https://keys.id.example/jwks",
            "https://id.example:8443/jwks",
            "http://id.example/jwks",
        ] {
            let error = OidcDiscoveryClient::require_issuer_origin(
                &issuer,
                &Url::parse(uri).expect("JWKS URI"),
            )
            .expect_err("a different origin must be refused");
            assert!(matches!(
                error,
                AuthError::Unavailable(ref message) if message.contains("issuer origin")
            ));
        }
    }
}
