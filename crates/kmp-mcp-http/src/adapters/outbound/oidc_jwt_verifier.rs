use std::sync::Arc;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{AlgorithmFamily, DecodingKey, Validation, decode, decode_header};
use tokio::sync::RwLock;
use url::Url;

use crate::adapters::outbound::oidc_discovery_client::OidcDiscoveryClient;
use crate::application::dto::claims_dto::ClaimsDto;
use crate::application::mappers::claims_mapper::ClaimsMapper;
use crate::domain::auth_error::AuthError;
use crate::domain::identity::Identity;
use crate::ports::token_verifier::{TokenVerifier, VerifyFuture};

#[derive(Clone)]
pub struct OidcJwtVerifier {
    discovery: OidcDiscoveryClient,
    issuer: String,
    audience: String,
    jwks_uri: Url,
    keys: Arc<RwLock<JwkSet>>,
}

impl OidcJwtVerifier {
    pub async fn discover(
        issuer: Url,
        audience: String,
        explicit_jwks_uri: Option<Url>,
    ) -> Result<Self, AuthError> {
        let discovery = OidcDiscoveryClient::new()?;
        let (jwks_uri, keys) = discovery.resolve_keys(&issuer, explicit_jwks_uri).await?;
        Ok(Self {
            discovery,
            issuer: issuer.as_str().to_string(),
            audience,
            jwks_uri,
            keys: Arc::new(RwLock::new(keys)),
        })
    }

    async fn key_for(&self, kid: &str) -> Result<jsonwebtoken::jwk::Jwk, AuthError> {
        if let Some(key) = self.keys.read().await.find(kid).cloned() {
            return Ok(key);
        }
        let refreshed = self.discovery.fetch_jwks(&self.jwks_uri).await?;
        let key = refreshed.find(kid).cloned();
        *self.keys.write().await = refreshed;
        key.ok_or_else(|| AuthError::Unauthorized("token key id is unknown".to_string()))
    }

    async fn verify_token(&self, token: &str) -> Result<Identity, AuthError> {
        let header = decode_header(token)
            .map_err(|_| AuthError::Unauthorized("bearer token is malformed".to_string()))?;
        if header.alg.family() == AlgorithmFamily::Hmac {
            return Err(AuthError::Unauthorized(
                "symmetric JWT algorithms are not accepted".to_string(),
            ));
        }
        let kid = header
            .kid
            .as_deref()
            .ok_or_else(|| AuthError::Unauthorized("bearer token has no key id".to_string()))?;
        let key = self.key_for(kid).await?;
        let decoding_key = DecodingKey::from_jwk(&key)
            .map_err(|_| AuthError::Unauthorized("token key is invalid".to_string()))?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[&self.issuer]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_nbf = true;
        let data = decode::<ClaimsDto>(token, &decoding_key, &validation)
            .map_err(|_| AuthError::Unauthorized("bearer token is invalid".to_string()))?;
        ClaimsMapper::to_identity(data.claims)
    }
}

impl TokenVerifier for OidcJwtVerifier {
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a> {
        Box::pin(self.verify_token(token))
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use jsonwebtoken::jwk::Jwk;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
    use serde::Serialize;
    use serde_json::json;

    use super::*;

    const PRIVATE_ED25519_DER: &str =
        "MC4CAQAwBQYDK2VwBCIEIGrD/e7uKYqSY4twDEsRfMMuLSrODf14dpTiTK6K1YI0";

    #[derive(Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        aud: &'a str,
        sub: &'a str,
        exp: u64,
        nbf: u64,
        scope: &'a str,
        kmp_abouts: Vec<&'a str>,
        kmp_scope_ids: Vec<&'a str>,
        kmp_ref_prefixes: Vec<&'a str>,
        workspace: &'a str,
    }

    fn verifier(audience: &str) -> OidcJwtVerifier {
        let jwk: Jwk = serde_json::from_value(json!({
            "kty": "OKP", "use": "sig", "crv": "Ed25519",
            "x": "2-Jj2UvNCvQiUPNYRgSi0cJSPiJI6Rs6D0UTeEpQVj8",
            "kid": "ed01", "alg": "EdDSA"
        }))
        .expect("test JWK");
        OidcJwtVerifier {
            discovery: OidcDiscoveryClient::new().expect("client"),
            issuer: "https://id.example/".to_string(),
            audience: audience.to_string(),
            jwks_uri: Url::parse("https://id.example/jwks").expect("JWKS URI"),
            keys: Arc::new(RwLock::new(JwkSet { keys: vec![jwk] })),
        }
    }

    fn token() -> String {
        let now = get_current_timestamp();
        let claims = TestClaims {
            iss: "https://id.example/",
            aud: "https://kmp.example/mcp",
            sub: "agent-7",
            exp: now + 300,
            nbf: now.saturating_sub(1),
            scope: "kmp:read kmp:inspect:raw",
            kmp_abouts: vec!["project:kmp"],
            kmp_scope_ids: vec!["timeline:kmp"],
            kmp_ref_prefixes: vec!["project:kmp:"],
            workspace: "underpass",
        };
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("ed01".to_string());
        let key = base64::engine::general_purpose::STANDARD
            .decode(PRIVATE_ED25519_DER)
            .expect("private key DER");
        encode(&header, &claims, &EncodingKey::from_ed_der(&key)).expect("signed token")
    }

    #[tokio::test]
    async fn verifies_asymmetric_oidc_claims_and_maps_kmp_grants() {
        let identity = verifier("https://kmp.example/mcp")
            .verify_token(&token())
            .await
            .expect("valid identity");
        assert_eq!(identity.subject, "agent-7");
        assert_eq!(identity.workspace.as_deref(), Some("underpass"));
        assert!(identity.scopes.contains("kmp:read"));
        assert!(identity.abouts.contains("project:kmp"));
    }

    #[tokio::test]
    async fn rejects_a_token_for_another_audience() {
        let error = verifier("https://other.example")
            .verify_token(&token())
            .await
            .expect_err("audience mismatch");
        assert!(matches!(error, AuthError::Unauthorized(_)));
    }
}
