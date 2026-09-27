//! The fingerprint salt of the store this gRPC API serves, kept on the
//! server beside it (`KMP_TELEMETRY_SALT_PATH`, or the embedded data
//! directory's `telemetry-salt`): read or created on the first Ask or Wake
//! that succeeds, never logged, never in a bundle. Without a path the API
//! logs no fingerprint.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use kmp_observability::FingerprintSalt;

#[derive(Debug, Default)]
pub(crate) struct GrpcCallTelemetry {
    salt_path: Option<PathBuf>,
    salt: OnceLock<Option<Arc<FingerprintSalt>>>,
}

impl GrpcCallTelemetry {
    pub(crate) fn with_salt_path(path: PathBuf) -> Self {
        Self {
            salt_path: Some(path),
            salt: OnceLock::new(),
        }
    }

    /// The salt, created only after a read succeeded (so the store surely
    /// answers). One that cannot be had is said once.
    pub(crate) fn salt(&self) -> Option<&FingerprintSalt> {
        self.salt
            .get_or_init(|| {
                let path = self.salt_path.as_ref()?;
                match FingerprintSalt::load_or_create(path) {
                    Ok(salt) => Some(Arc::new(salt)),
                    Err(error) => {
                        tracing::warn!(
                            event = "kmp_telemetry_salt",
                            %error,
                            "call fingerprints are off for this server"
                        );
                        None
                    }
                }
            })
            .as_deref()
    }

    /// The keyed fingerprint of a question or an intent, as the MCP server
    /// computes it for the same store.
    pub(crate) fn fingerprint_text(&self, domain: &str, text: &str) -> Option<String> {
        self.salt()?.fingerprint_text(domain, text)
    }
}

#[cfg(test)]
mod tests {
    use kmp_observability::{FingerprintSalt, TELEMETRY_SALT_FILE};

    use super::GrpcCallTelemetry;

    #[test]
    fn fingerprints_only_with_a_salt_and_as_the_mcp_server_would() {
        assert_eq!(
            GrpcCallTelemetry::default().fingerprint_text("question", "q"),
            None
        );

        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(TELEMETRY_SALT_FILE);
        let telemetry = GrpcCallTelemetry::with_salt_path(path.clone());
        let print = telemetry
            .fingerprint_text("question", "Who  approved it?")
            .expect("print");
        let same_store = FingerprintSalt::load_or_create(&path).expect("salt");
        assert_eq!(print.len(), 64);
        assert_eq!(
            Some(print),
            same_store.fingerprint_text("question", "who approved it?")
        );
        assert_eq!(print_len(&telemetry), 64);

        let broken = GrpcCallTelemetry::with_salt_path(dir.path().join("absent").join("salt"));
        assert_eq!(broken.fingerprint_text("question", "q"), None);
    }

    fn print_len(telemetry: &GrpcCallTelemetry) -> usize {
        telemetry
            .fingerprint_text("intent", "resume")
            .map_or(0, |print| print.len())
    }
}
