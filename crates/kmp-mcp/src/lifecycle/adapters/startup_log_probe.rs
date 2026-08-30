use std::path::{Path, PathBuf};

/// The last startup outcomes this data directory recorded, newest last.
pub(crate) fn startup_history(data_dir: &Path, limit: usize) -> Vec<String> {
    // The log rolls, so the name on disk carries a date suffix. Reading only
    // `kmp-mcp.log` finds nothing on every day but the first.
    let Ok(entries) = std::fs::read_dir(data_dir.join("logs")) else {
        return Vec::new();
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("kmp-mcp.log"))
        })
        .collect();
    logs.sort();
    let text: String = logs
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| line.contains("startup succeeded") || line.contains("startup failed"))
        .map(|line| {
            let outcome = if line.contains("startup failed") {
                "failed"
            } else {
                "ok"
            };
            let stamp = line
                .split('"')
                .find(|part| part.len() >= 19 && part.starts_with("20"))
                .unwrap_or("")
                .replace('T', " ");
            format!("{outcome:<7}{}", &stamp[..stamp.len().min(19)])
        })
        .collect();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::startup_history;

    #[test]
    fn the_startup_history_reads_a_rolled_log() {
        // The log rolls daily, so the file on disk carries a date suffix.
        // Reading only `kmp-mcp.log` finds nothing on every day but the first.
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        std::fs::write(
            logs.join("kmp-mcp.log.2026-08-23"),
            "{\"timestamp\":\"2026-08-23T19:12:46.0Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n\
             {\"timestamp\":\"2026-08-23T19:27:42.0Z\",\"fields\":{\"message\":\"startup failed\"}}\n",
        )
        .expect("write log");

        let history = startup_history(dir.path(), 5);
        assert_eq!(history.len(), 2, "{history:?}");
        assert!(history[0].starts_with("ok"), "{history:?}");
        assert!(history[1].starts_with("failed"), "{history:?}");
        assert!(history[1].contains("2026-08-23 19:27:42"), "{history:?}");
    }
    #[test]
    fn the_history_keeps_only_the_newest_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        let mut text = String::new();
        for minute in 0..9 {
            text.push_str(&format!(
                "{{\"timestamp\":\"2026-08-23T19:0{minute}:00.0Z\",\"fields\":{{\"message\":\"startup succeeded\"}}}}\n"
            ));
        }
        std::fs::write(logs.join("kmp-mcp.log.2026-08-23"), text).expect("write log");

        let history = startup_history(dir.path(), 3);
        assert_eq!(history.len(), 3);
        assert!(
            history[2].contains("19:08:00"),
            "the newest survives: {history:?}"
        );
    }
    #[test]
    fn a_missing_log_directory_is_silence_rather_than_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(startup_history(dir.path(), 5).is_empty());
    }
}
