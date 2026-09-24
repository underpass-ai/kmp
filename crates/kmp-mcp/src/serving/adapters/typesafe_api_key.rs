use reqwest::header::HeaderValue;

/// The TypeSafe bearer credential. Read from the environment only; never
/// serialized, and redacted from `Debug`.
pub(super) struct TypeSafeApiKey(HeaderValue);

impl TypeSafeApiKey {
    pub(super) fn from_env(value: Option<String>) -> Result<Self, String> {
        let key = value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "TYPESAFE_API_KEY is not set".to_string())?;
        if key.len() > 512 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("TYPESAFE_API_KEY is malformed".into());
        }
        let mut header = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| "TYPESAFE_API_KEY is malformed")?;
        header.set_sensitive(true);
        Ok(Self(header))
    }

    pub(super) fn header(&self) -> HeaderValue {
        self.0.clone()
    }
}

impl std::fmt::Debug for TypeSafeApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TypeSafeApiKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_malformed_key_is_refused_without_echoing_it() {
        assert!(TypeSafeApiKey::from_env(None).is_err());
        assert!(TypeSafeApiKey::from_env(Some(String::new())).is_err());
        for bad in ["has space", "line\nbreak"] {
            let error = TypeSafeApiKey::from_env(Some(bad.into())).expect_err("refused");
            assert!(!error.contains(bad), "echoed the key");
        }
    }

    #[test]
    fn the_key_is_a_sensitive_bearer_header_and_debug_redacts_it() {
        let key = TypeSafeApiKey::from_env(Some("apikey_secret".into())).expect("key");
        let header = key.header();
        assert!(header.is_sensitive());
        assert_eq!(header.to_str().expect("ascii"), "Bearer apikey_secret");
        assert!(!format!("{key:?}").contains("apikey_secret"));
    }
}
