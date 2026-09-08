use serde::Deserialize;

/// Explicit per-store opt-in for a local retrieval sidecar.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticRetrieverConfig {
    pub endpoint: String,
    pub model_revision: String,
}

impl SemanticRetrieverConfig {
    pub(super) fn validate(&self) -> Result<reqwest::Url, String> {
        let url = reqwest::Url::parse(&self.endpoint).map_err(|_| "invalid semantic endpoint")?;
        let local = url
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if url.scheme() != "http"
            || !local
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.model_revision.trim().is_empty()
            || self.model_revision.len() > 256
        {
            return Err(
                "semantic retrieval requires a literal loopback HTTP endpoint and model revision"
                    .into(),
            );
        }
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_literal_loopback_without_credentials_or_redirects_is_configurable() {
        for endpoint in ["http://127.0.0.1:8001/rank", "http://[::1]:8001/rank"] {
            assert!(
                SemanticRetrieverConfig {
                    endpoint: endpoint.into(),
                    model_revision: "model@revision".into()
                }
                .validate()
                .is_ok()
            );
        }
        for endpoint in [
            "http://localhost:8001/rank",
            "http://192.168.0.1/rank",
            "https://example.com/rank",
            "http://user:password@127.0.0.1/rank",
            "http://127.0.0.1/rank?target=other",
        ] {
            assert!(
                SemanticRetrieverConfig {
                    endpoint: endpoint.into(),
                    model_revision: "model@revision".into()
                }
                .validate()
                .is_err()
            );
        }
    }
}
