use std::time::Duration;

use serde::Deserialize;

/// Explicit per-store opt-in for TypeSafe judgement. Holds no credential.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeSafeConfig {
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
}

impl TypeSafeConfig {
    pub(super) fn validate(&self) -> Result<(reqwest::Url, Duration), String> {
        let url = reqwest::Url::parse(&self.endpoint).map_err(|_| "invalid TypeSafe endpoint")?;
        if url.scheme() != "https"
            || url.host_str() != Some("api.typesafe.ai")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "TypeSafe endpoint must be https://api.typesafe.ai without credentials, port, query or fragment"
                    .into(),
            );
        }
        let model = self.model.as_str();
        if model.is_empty()
            || model.len() > 64
            || model.ends_with("-latest")
            || !model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        {
            return Err("TypeSafe model must be a pinned version such as jev-1.13.0".into());
        }
        if !(1_000..=60_000).contains(&self.timeout_ms) {
            return Err("TypeSafe timeout_ms must be between 1000 and 60000".into());
        }
        Ok((url, Duration::from_millis(self.timeout_ms)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(endpoint: &str, model: &str, timeout_ms: u64) -> TypeSafeConfig {
        TypeSafeConfig {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout_ms,
        }
    }

    const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";

    #[test]
    fn only_the_provider_host_a_pinned_model_and_a_bounded_timeout_validate() {
        let (url, timeout) = config(ENDPOINT, "jev-1.13.0", 20_000)
            .validate()
            .expect("valid");
        assert_eq!(url.as_str(), ENDPOINT);
        assert_eq!(timeout, Duration::from_millis(20_000));
        for bad in [
            config("http://api.typesafe.ai/v1/systemone", "jev-1.13.0", 20_000),
            config("https://example.com/v1/systemone", "jev-1.13.0", 20_000),
            config(
                "https://api.typesafe.ai:8443/v1/systemone",
                "jev-1.13.0",
                20_000,
            ),
            config(
                "https://u:p@api.typesafe.ai/v1/systemone",
                "jev-1.13.0",
                20_000,
            ),
            config(
                "https://api.typesafe.ai/v1/systemone?x=1",
                "jev-1.13.0",
                20_000,
            ),
            config(ENDPOINT, "jev-latest", 20_000),
            config(ENDPOINT, "", 20_000),
            config(ENDPOINT, "jev 1.13", 20_000),
            config(ENDPOINT, "jev-1.13.0", 999),
            config(ENDPOINT, "jev-1.13.0", 60_001),
        ] {
            assert!(bad.validate().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let parsed = serde_json::from_str::<TypeSafeConfig>(
            r#"{"endpoint":"x","model":"y","timeout_ms":1000,"api_key":"z"}"#,
        );
        assert!(parsed.is_err());
    }
}
