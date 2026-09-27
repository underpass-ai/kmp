//! Where this server keeps the salt that keys its Ask and Wake log
//! fingerprints: `KMP_TELEMETRY_SALT_PATH` when set, otherwise
//! `telemetry-salt` in the server's data directory — `KMP_DATA_DIR`, or
//! `$XDG_DATA_HOME/kmp/server` (`~/.local/share/kmp/server`). The served store
//! is remote; this directory is the server's own, beside it, and the salt
//! never leaves it.

use std::path::PathBuf;

/// The salt file and, for the default location, the data directory the
/// server may create for it (never the directory of an explicit path).
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct TelemetrySaltLocation {
    pub(crate) salt: PathBuf,
    pub(crate) data_dir: Option<PathBuf>,
}

impl TelemetrySaltLocation {
    pub(crate) fn resolve(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let set = |key: &str| lookup(key).filter(|value| !value.trim().is_empty());
        if let Some(path) = set("KMP_TELEMETRY_SALT_PATH") {
            return Some(Self {
                salt: PathBuf::from(path),
                data_dir: None,
            });
        }
        let data_dir = match set("KMP_DATA_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => set("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".local/share")))?
                .join("kmp/server"),
        };
        Some(Self {
            salt: data_dir.join(kmp_observability::TELEMETRY_SALT_FILE),
            data_dir: Some(data_dir),
        })
    }

    /// The salt path, after making sure the server's own data directory
    /// exists; an explicit path's directory is the operator's to provide.
    pub(crate) fn prepare(self) -> PathBuf {
        if let Some(dir) = &self.data_dir
            && let Err(error) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(
                event = "kmp_telemetry_salt",
                error = %error.kind(),
                "the server data directory cannot be created; call fingerprints are off"
            );
        }
        self.salt
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::TelemetrySaltLocation;

    fn resolve(pairs: &[(&str, &str)]) -> Option<TelemetrySaltLocation> {
        let env: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        TelemetrySaltLocation::resolve(|key| env.get(key).cloned())
    }

    #[test]
    fn the_explicit_path_wins_then_the_data_dir_then_the_user_data_home() {
        let explicit = resolve(&[
            ("KMP_TELEMETRY_SALT_PATH", "/secrets/salt"),
            ("KMP_DATA_DIR", "/var/lib/kmp"),
        ])
        .expect("explicit");
        assert_eq!(explicit.salt, PathBuf::from("/secrets/salt"));
        assert_eq!(explicit.data_dir, None);

        let data_dir =
            resolve(&[("KMP_DATA_DIR", "/var/lib/kmp"), ("HOME", "/home/k")]).expect("data dir");
        assert_eq!(data_dir.salt, PathBuf::from("/var/lib/kmp/telemetry-salt"));

        let xdg = resolve(&[("XDG_DATA_HOME", "/x"), ("HOME", "/home/k")]).expect("xdg");
        assert_eq!(xdg.salt, PathBuf::from("/x/kmp/server/telemetry-salt"));
        let home = resolve(&[("HOME", "/home/k"), ("KMP_TELEMETRY_SALT_PATH", " ")]).expect("home");
        assert_eq!(
            home.salt,
            PathBuf::from("/home/k/.local/share/kmp/server/telemetry-salt")
        );
        assert_eq!(resolve(&[]), None);
    }

    #[test]
    fn prepare_creates_only_the_servers_own_directory() {
        let base = tempfile::tempdir().expect("dir");
        let own = base.path().join("kmp/server");
        let location = TelemetrySaltLocation {
            salt: own.join("telemetry-salt"),
            data_dir: Some(own.clone()),
        };
        assert_eq!(location.prepare(), own.join("telemetry-salt"));
        assert!(own.is_dir());

        let explicit = base.path().join("operator/salt");
        let location = TelemetrySaltLocation {
            salt: explicit.clone(),
            data_dir: None,
        };
        assert_eq!(location.prepare(), explicit);
        assert!(!base.path().join("operator").exists());
    }
}
