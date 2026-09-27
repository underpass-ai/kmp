//! The random key a store fingerprints questions and contexts with, kept
//! beside the store and nowhere else. Shared by the MCP server and the gRPC
//! API so both log comparable fingerprints for one store.

use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use super::hmac_sha256::hmac_sha256;

const SALT_BYTES: usize = 32;

/// The file beside a store's configuration, outside `store/` like the
/// verdict book: 32 random bytes, mode 0600, created on first use. It never
/// enters a log, a bundle or a network request; deleting it only makes new
/// fingerprints incomparable with old ones.
pub const TELEMETRY_SALT_FILE: &str = "telemetry-salt";

/// A per-store HMAC-SHA256 key. Fingerprints under it compare within the
/// store and with nothing else: not across stores, not across machines.
pub struct FingerprintSalt {
    key: [u8; SALT_BYTES],
}

impl FingerprintSalt {
    /// Reads the salt at `path`, or creates it there when absent. The
    /// directory must exist: a salt never creates a store. Concurrent first
    /// uses agree on one salt — the file appears whole or not at all.
    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        match Self::load(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            other => return other.map_err(|error| salt_error("read", &error)),
        }
        let key = random_key()?;
        let staged = staged_path(path, &key);
        write_private(&staged, &key).map_err(|error| salt_error("write", &error))?;
        let linked = std::fs::hard_link(&staged, path);
        let _ = std::fs::remove_file(&staged);
        match linked {
            Ok(()) => Ok(Self { key }),
            // Another process won the race: its salt is the store's.
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                Self::load(path).map_err(|error| salt_error("read", &error))
            }
            Err(error) => Err(salt_error("write", &error)),
        }
    }

    fn load(path: &Path) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut key = [0u8; SALT_BYTES];
        file.read_exact(&mut key)?;
        let mut rest = [0u8; 1];
        if file.read(&mut rest)? != 0 {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "longer than 32 bytes",
            ));
        }
        Ok(Self { key })
    }

    /// The keyed fingerprint of `text` under `domain`, the whole
    /// HMAC-SHA256 as 64 lower-case hex digits. `domain` separates what was
    /// fingerprinted: a question and a context id with the same bytes never
    /// share a fingerprint.
    pub fn fingerprint(&self, domain: &str, text: &str) -> String {
        let digest = hmac_sha256(&self.key, &[domain.as_bytes(), b"\0", text.as_bytes()]);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// The fingerprint of a question or an intent as a person would repeat
    /// it: Unicode NFC, lower case and runs of whitespace as one space;
    /// punctuation is kept.
    pub fn fingerprint_text(&self, domain: &str, text: &str) -> Option<String> {
        let normalized = normalized_text(text);
        (!normalized.is_empty()).then(|| self.fingerprint(domain, &normalized))
    }
}

impl fmt::Debug for FingerprintSalt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FingerprintSalt(<redacted>)")
    }
}

fn normalized_text(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfc()
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn random_key() -> Result<[u8; SALT_BYTES], String> {
    let mut key = [0u8; SALT_BYTES];
    getrandom::getrandom(&mut key)
        .map_err(|error| format!("telemetry salt: no randomness: {error}"))?;
    Ok(key)
}

/// A name no other process stages under: the salt's own first bytes.
fn staged_path(path: &Path, key: &[u8; SALT_BYTES]) -> PathBuf {
    let tag: String = key[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{}.{tag}.tmp", std::process::id()));
    path.with_file_name(name)
}

fn write_private(path: &Path, key: &[u8; SALT_BYTES]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    let written = file.write_all(key).and_then(|()| file.sync_all());
    if written.is_err() {
        let _ = std::fs::remove_file(path);
    }
    written
}

fn salt_error(action: &str, error: &std::io::Error) -> String {
    format!("telemetry salt: cannot {action} it: {}", error.kind())
}

#[cfg(test)]
mod tests {
    use super::{FingerprintSalt, TELEMETRY_SALT_FILE};

    #[test]
    fn first_use_creates_a_private_salt_that_later_uses_reuse() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(TELEMETRY_SALT_FILE);

        let first = FingerprintSalt::load_or_create(&path).expect("created");
        let again = FingerprintSalt::load_or_create(&path).expect("read");

        assert_eq!(std::fs::read(&path).expect("salt").len(), 32);
        assert_eq!(
            first.fingerprint("question", "why?"),
            again.fingerprint("question", "why?")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from(TELEMETRY_SALT_FILE)]
        );
    }

    #[test]
    fn fingerprints_separate_domains_and_stores_and_hide_the_text() {
        let one = tempfile::tempdir().expect("dir");
        let two = tempfile::tempdir().expect("dir");
        let a = FingerprintSalt::load_or_create(&one.path().join(TELEMETRY_SALT_FILE)).expect("a");
        let b = FingerprintSalt::load_or_create(&two.path().join(TELEMETRY_SALT_FILE)).expect("b");

        let print = a.fingerprint("question", "who approved the rollback");
        assert_eq!(print.len(), 64);
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(print, a.fingerprint("context", "who approved the rollback"));
        assert_ne!(
            print,
            b.fingerprint("question", "who approved the rollback")
        );
        assert!(!format!("{a:?}").contains(&print));
    }

    #[test]
    fn text_fingerprints_ignore_case_spacing_and_composition_but_not_punctuation() {
        let dir = tempfile::tempdir().expect("dir");
        let salt =
            FingerprintSalt::load_or_create(&dir.path().join(TELEMETRY_SALT_FILE)).expect("salt");
        let composed = salt.fingerprint_text("question", "¿Quién aprobó el cambio?");
        let decomposed =
            salt.fingerprint_text("question", "  ¿QUIE\u{301}N  aprobo\u{301}\tel cambio? ");
        assert_eq!(composed, decomposed);
        assert!(composed.is_some());
        assert_ne!(
            composed,
            salt.fingerprint_text("question", "Quién aprobó el cambio")
        );
        assert_eq!(salt.fingerprint_text("question", " \n "), None);
    }

    #[test]
    fn concurrent_first_uses_agree_on_one_salt() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(TELEMETRY_SALT_FILE);
        let prints: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        FingerprintSalt::load_or_create(&path)
                            .expect("salt")
                            .fingerprint("question", "q")
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("join"))
                .collect()
        });
        assert!(prints.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn a_missing_directory_or_a_damaged_salt_is_an_error_not_a_new_salt() {
        let dir = tempfile::tempdir().expect("dir");
        let missing = dir.path().join("absent").join(TELEMETRY_SALT_FILE);
        assert!(FingerprintSalt::load_or_create(&missing).is_err());
        assert!(!dir.path().join("absent").exists());

        let short = dir.path().join(TELEMETRY_SALT_FILE);
        std::fs::write(&short, b"short").expect("write");
        assert!(FingerprintSalt::load_or_create(&short).is_err());
        std::fs::write(&short, [7u8; 33]).expect("write");
        assert!(FingerprintSalt::load_or_create(&short).is_err());
        assert_eq!(std::fs::read(&short).expect("kept").len(), 33);
    }
}
