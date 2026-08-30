use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub(crate) fn telemetry_finding(resolved: &ResolvedDataDir) -> LifecycleFinding {
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
