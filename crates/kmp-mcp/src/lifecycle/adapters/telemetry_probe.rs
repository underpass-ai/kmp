use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub(crate) fn telemetry_finding(resolved: &ResolvedDataDir) -> LifecycleFinding {
    quality_finding(resolved).with_detail(salt_detail(resolved.path()))
}

/// Whether the store's call-fingerprint salt exists — never what it holds.
fn salt_detail(data_dir: &std::path::Path) -> String {
    let path = data_dir.join(kmp_observability::TELEMETRY_SALT_FILE);
    if path.is_file() {
        format!(
            "call fingerprint salt present at {} (delete it to rotate; older fingerprints stop comparing)",
            path.display()
        )
    } else {
        "call fingerprint salt not created yet — the first wake or ask that succeeds creates it"
            .to_string()
    }
}

fn quality_finding(resolved: &ResolvedDataDir) -> LifecycleFinding {
    let path = kmp_embedded::quality_telemetry_path(resolved.path());
    if !path.exists() {
        return LifecycleFinding::new(DiagnosticSeverity::Warn, "no quality telemetry journal yet")
            .with_detail(format!(
                "expected at {} after the first kernel start",
                path.display()
            ));
    }
    match kmp_embedded::SqliteQualityTelemetryReader::open(resolved.path()) {
        Ok(reader) => match reader.count() {
            Ok(count) => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!("quality pulse readable · {count} observations"),
            )
            .with_detail(path.display().to_string()),
            Err(error) => {
                LifecycleFinding::new(DiagnosticSeverity::Warn, "quality telemetry cannot be read")
                    .with_detail(error.to_string())
            }
        },
        Err(error) => {
            let raw = error.to_string();
            let headline = if raw.contains("Cannot acquire lock")
                || raw.to_ascii_lowercase().contains("already open")
            {
                "quality telemetry is held by another process"
            } else {
                "quality telemetry is unavailable"
            };
            LifecycleFinding::new(DiagnosticSeverity::Warn, headline).with_detail(raw)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::salt_detail;

    #[test]
    fn the_salt_is_reported_by_presence_never_by_content() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(salt_detail(dir.path()).contains("not created yet"));
        let path = dir.path().join(kmp_observability::TELEMETRY_SALT_FILE);
        std::fs::write(&path, [0xabu8; 32]).expect("salt");
        let detail = salt_detail(dir.path());
        assert!(detail.contains("present"));
        assert!(!detail.contains("abab"));
    }
}
