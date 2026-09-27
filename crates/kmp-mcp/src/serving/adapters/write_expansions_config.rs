use std::path::Path;

use serde::Deserialize;

/// The file beside the store that opts its writes into judged search
/// expansions (P15, Doc2Query--).
pub(crate) const WRITE_EXPANSIONS_FILE: &str = "write-expansions.json";

/// The per-store opt-in for search expansions. It rides on the store's
/// `typesafe.json`: without a judge nothing is stored. `{}` turns it on with
/// the bar fixed on the development corpora.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteExpansionsConfig {
    /// An expansion Jev reads as belonging to its memory with at least this
    /// probability is kept; the rest are refused.
    #[serde(default = "default_accept_at")]
    accept_at: f64,
    /// How much of the memory the judge reads.
    #[serde(default = "default_excerpt_chars")]
    excerpt_chars: usize,
}

/// Fixed on development (P15 pre-registration `kmp.p15.prereg.v1`).
fn default_accept_at() -> f64 {
    0.5
}

fn default_excerpt_chars() -> usize {
    1_500
}

impl Default for WriteExpansionsConfig {
    fn default() -> Self {
        Self {
            accept_at: default_accept_at(),
            excerpt_chars: default_excerpt_chars(),
        }
    }
}

impl WriteExpansionsConfig {
    /// `None` without the file; an error names why a present file cannot
    /// apply, and then nothing is stored.
    pub(crate) fn load(data_dir: &Path) -> Result<Option<Self>, String> {
        let bytes = match std::fs::read(data_dir.join(WRITE_EXPANSIONS_FILE)) {
            Ok(bytes) if bytes.len() <= 4096 => bytes,
            Ok(_) => return Err("write expansions configuration exceeds 4096 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read write expansions configuration".into()),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid write expansions configuration".to_string())?;
        if !(config.accept_at.is_finite() && config.accept_at > 0.0 && config.accept_at <= 1.0) {
            return Err("write-expansions accept_at must be in (0, 1]".into());
        }
        if !(200..=4_000).contains(&config.excerpt_chars) {
            return Err("write-expansions excerpt_chars must be between 200 and 4000".into());
        }
        Ok(Some(config))
    }

    pub(crate) fn accept_at(&self) -> f64 {
        self.accept_at
    }

    pub(crate) fn excerpt_chars(&self) -> usize {
        self.excerpt_chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(body: Option<&str>) -> Result<Option<WriteExpansionsConfig>, String> {
        let dir = tempfile::tempdir().expect("dir");
        if let Some(body) = body {
            std::fs::write(dir.path().join(WRITE_EXPANSIONS_FILE), body).expect("write");
        }
        WriteExpansionsConfig::load(dir.path())
    }

    #[test]
    fn the_file_is_the_opt_in_and_its_bar_is_checked() {
        assert_eq!(load(None), Ok(None));
        let config = load(Some("{}")).expect("valid").expect("opted in");
        assert_eq!(config, WriteExpansionsConfig::default());
        assert_eq!(config.excerpt_chars(), 1_500);
        let config = load(Some(r#"{"accept_at":0.8}"#))
            .expect("valid")
            .expect("in");
        assert_eq!(config.accept_at(), 0.8);
        assert!(load(Some(r#"{"accept_at":0}"#)).is_err());
        assert!(load(Some(r#"{"accept_at":1.5}"#)).is_err());
        assert!(load(Some(r#"{"excerpt_chars":10}"#)).is_err());
        assert!(load(Some(r#"{"other":1}"#)).is_err());
        assert!(load(Some("not json")).is_err());
        assert!(load(Some(&" ".repeat(5000))).is_err());
    }
}
