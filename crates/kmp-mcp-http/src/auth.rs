use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{AlgorithmFamily, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;

const MAX_DISCOVERY_BYTES: usize = 256 * 1024;
const MAX_JWKS_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub subject: String,
    pub workspace: Option<String>,
    pub scopes: BTreeSet<String>,
    pub abouts: BTreeSet<String>,
    pub scope_ids: BTreeSet<String>,
    pub ref_prefixes: BTreeSet<String>,
}

impl Identity {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
    Unauthorized(String),
    Unavailable(String),
}

pub type VerifyFuture<'a> = Pin<Box<dyn Future<Output = Result<Identity, AuthError>> + Send + 'a>>;

pub trait TokenVerifier: Send + Sync {
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a>;
}

#[derive(Clone)]
pub struct OidcJwtVerifier {
    client: reqwest::Client,
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
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| AuthError::Unavailable(format!("OIDC client: {error}")))?;
        let jwks_uri = match explicit_jwks_uri {
            Some(uri) => uri,
            None => discover_jwks_uri(&client, &issuer).await?,
        };
        if jwks_uri.scheme() != "https" {
            return Err(AuthError::Unavailable(
                "OIDC jwks_uri must use https".to_string(),
            ));
        }
        require_issuer_origin(&issuer, &jwks_uri)?;
        let keys = fetch_jwks(&client, &jwks_uri).await?;
        Ok(Self {
            client,
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

        let refreshed = fetch_jwks(&self.client, &self.jwks_uri).await?;
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
        let data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|_| AuthError::Unauthorized("bearer token is invalid".to_string()))?;
        data.claims.into_identity()
    }
}

impl TokenVerifier for OidcJwtVerifier {
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a> {
        Box::pin(self.verify_token(token))
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    scope: StringOrList,
    #[serde(default)]
    kmp_abouts: StringOrList,
    #[serde(default)]
    kmp_scope_ids: StringOrList,
    #[serde(default)]
    kmp_ref_prefixes: StringOrList,
}

impl Claims {
    fn into_identity(self) -> Result<Identity, AuthError> {
        if self.sub.trim().is_empty() {
            return Err(AuthError::Unauthorized(
                "bearer token subject is empty".to_string(),
            ));
        }
        Ok(Identity {
            subject: self.sub,
            workspace: self.workspace,
            scopes: self.scope.into_set(true),
            abouts: self.kmp_abouts.into_set(false),
            scope_ids: self.kmp_scope_ids.into_set(false),
            ref_prefixes: self.kmp_ref_prefixes.into_set(false),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum StringOrList {
    #[default]
    Missing,
    String(String),
    List(Vec<String>),
}

impl StringOrList {
    fn into_set(self, split_spaces: bool) -> BTreeSet<String> {
        match self {
            Self::Missing => BTreeSet::new(),
            Self::String(value) if split_spaces => value
                .split_ascii_whitespace()
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
            Self::String(value) => value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
            Self::List(values) => values
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        }
    }
}

#[derive(Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    jwks_uri: String,
}

async fn discover_jwks_uri(client: &reqwest::Client, issuer: &Url) -> Result<Url, AuthError> {
    let mut discovery_url = issuer.clone();
    let path = format!(
        "{}/.well-known/openid-configuration",
        issuer.path().trim_end_matches('/')
    );
    discovery_url.set_path(&path);
    discovery_url.set_query(None);
    discovery_url.set_fragment(None);
    let document: DiscoveryDocument =
        fetch_json(client, discovery_url, MAX_DISCOVERY_BYTES).await?;
    if document.issuer != issuer.as_str() {
        return Err(AuthError::Unavailable(
            "OIDC discovery issuer does not match configured issuer".to_string(),
        ));
    }
    Url::parse(&document.jwks_uri)
        .map_err(|error| AuthError::Unavailable(format!("OIDC jwks_uri is invalid: {error}")))
}

async fn fetch_jwks(client: &reqwest::Client, uri: &Url) -> Result<JwkSet, AuthError> {
    fetch_json(client, uri.clone(), MAX_JWKS_BYTES).await
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
    client: &reqwest::Client,
    uri: Url,
    max_bytes: usize,
) -> Result<T, AuthError> {
    let mut response = client
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
            "kty": "OKP",
            "use": "sig",
            "crv": "Ed25519",
            "x": "2-Jj2UvNCvQiUPNYRgSi0cJSPiJI6Rs6D0UTeEpQVj8",
            "kid": "ed01",
            "alg": "EdDSA"
        }))
        .expect("test JWK");
        OidcJwtVerifier {
            client: reqwest::Client::new(),
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
        assert!(identity.scope_ids.contains("timeline:kmp"));
        assert!(identity.ref_prefixes.contains("project:kmp:"));
    }

    #[tokio::test]
    async fn rejects_a_token_for_another_audience() {
        let error = verifier("https://other.example")
            .verify_token(&token())
            .await
            .expect_err("audience mismatch");
        assert!(matches!(error, AuthError::Unauthorized(_)));
    }

    #[test]
    fn jwks_uri_cannot_leave_the_operator_configured_issuer_origin() {
        let issuer = Url::parse("https://id.example/tenant").expect("issuer");
        for uri in [
            "https://keys.id.example/jwks",
            "https://id.example:8443/jwks",
            "http://id.example/jwks",
        ] {
            let error = require_issuer_origin(&issuer, &Url::parse(uri).expect("JWKS URI"))
                .expect_err("a different origin must be refused");
            assert!(
                matches!(error, AuthError::Unavailable(ref message) if message.contains("issuer origin")),
                "{error:?}"
            );
        }

        require_issuer_origin(
            &issuer,
            &Url::parse("https://id.example/tenant/keys.json").expect("same-origin JWKS URI"),
        )
        .expect("the issuer may serve keys at any same-origin path");
    }
}
