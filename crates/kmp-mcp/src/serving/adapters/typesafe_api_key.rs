use reqwest::header::HeaderValue;

/// The TypeSafe bearer credential. Read from the environment, or from the
/// owner-only `typesafe.env` in the user's config directory; never
/// serialized, and redacted from `Debug`.
pub(super) struct TypeSafeApiKey(HeaderValue);

/// The most a key file may hold: a key and a comment or two.
const KEY_FILE_BYTES: u64 = 4096;

impl TypeSafeApiKey {
    /// The key from the environment when set, else from the key file. A host
    /// launched from a desktop session never reads a shell profile, so the
    /// file is how such a host finds the key without it being copied into
    /// every host's configuration.
    pub(super) fn load(value: Option<String>) -> Result<Self, String> {
        Self::load_from(value, default_key_file())
    }

    /// `load` with the key file named: the default one in production, a
    /// temporary one in a test, so a key on the developer's machine never
    /// changes what a test sees.
    pub(super) fn load_from(
        value: Option<String>,
        key_file: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        if value
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Self::from_env(value);
        }
        match key_file {
            Some(path) if path.exists() => Self::from_file(&path),
            _ => Err(
                "TYPESAFE_API_KEY is not set and no ~/.config/typesafe.env holds it".to_string(),
            ),
        }
    }

    /// `TYPESAFE_API_KEY=<key>` from a file only its owner can read, the
    /// shape a shell `set -a; . file` also loads. Other lines are ignored.
    pub(super) fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|_| format!("cannot read the TypeSafe key file {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(format!(
                    "the TypeSafe key file {} is readable by others; chmod 600 it",
                    path.display()
                ));
            }
        }
        if metadata.len() > KEY_FILE_BYTES {
            return Err(format!(
                "the TypeSafe key file {} is larger than a key file",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|_| format!("cannot read the TypeSafe key file {}", path.display()))?;
        let value = text.lines().find_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
            line.strip_prefix("TYPESAFE_API_KEY=").map(|value| {
                value
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_string()
            })
        });
        Self::from_env(value).map_err(|error| format!("{error} in {}", path.display()))
    }

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

/// `$XDG_CONFIG_HOME/typesafe.env`, else `~/.config/typesafe.env`.
fn default_key_file() -> Option<std::path::PathBuf> {
    let non_empty = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    non_empty("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| non_empty("HOME").map(|home| std::path::PathBuf::from(home).join(".config")))
        .map(|config| config.join("typesafe.env"))
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

    #[test]
    fn a_key_file_is_read_only_when_its_owner_alone_can_read_it() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("typesafe.env");
        std::fs::write(
            &path,
            "# TypeSafe\nexport TYPESAFE_API_KEY=\"apikey_file\"\nOTHER=1\n",
        )
        .expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("mode");
            let error = TypeSafeApiKey::from_file(&path).expect_err("shared file refused");
            assert!(error.contains("chmod 600"), "{error}");
            assert!(!error.contains("apikey_file"), "echoed the key");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        let key = TypeSafeApiKey::from_file(&path).expect("key");
        assert_eq!(key.header().to_str().expect("ascii"), "Bearer apikey_file");
        std::fs::write(&path, "OTHER=1\n").expect("write");
        let error = TypeSafeApiKey::from_file(&path).expect_err("no key line");
        assert!(error.contains("not set"), "{error}");
    }

    #[test]
    fn the_environment_wins_over_the_file() {
        let key = TypeSafeApiKey::load(Some("apikey_env".into())).expect("key");
        assert_eq!(key.header().to_str().expect("ascii"), "Bearer apikey_env");
    }
}
