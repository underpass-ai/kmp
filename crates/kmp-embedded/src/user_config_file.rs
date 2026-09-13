//! The single file this machine's user-scope KMP settings live in.
//!
//! Two questions read it and neither can wait for the other: which memory to
//! open, asked here before any host exists, and how an agent is told to reach
//! for memory, asked by the MCP crate above this one. Both need the same
//! three answers — where the file is, what one root-level setting says, and
//! how to replace one setting without touching another byte — so the file
//! owns them once instead of being parsed twice with two sets of rules.
//!
//! It is a deliberate TOML subset: root-level `key = value` lines, `#`
//! comments, and tables that are read past rather than entered. A key inside
//! `[table]` belongs to that table and is never mistaken for a root setting.
//! Nothing in the file is ever rewritten except the one setting named.

use std::path::{Path, PathBuf};

/// Where the user's settings live, per platform.
pub fn user_config_path() -> Result<PathBuf, String> {
    let path_from = |name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    user_config_path_from(
        path_from("XDG_CONFIG_HOME"),
        path_from("HOME"),
        path_from("APPDATA"),
        path_from("USERPROFILE"),
    )
}

pub(crate) fn user_config_path_from(
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
    app_data: Option<PathBuf>,
    user_profile: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(root) = xdg_config_home {
        return Ok(root.join("kmp").join("config.toml"));
    }
    if let Some(root) = home {
        return Ok(root.join(".config").join("kmp").join("config.toml"));
    }
    if let Some(root) = app_data {
        return Ok(root.join("kmp").join("config.toml"));
    }
    user_profile
        .map(|root| root.join(".config").join("kmp").join("config.toml"))
        .ok_or_else(|| {
            "none of XDG_CONFIG_HOME, HOME, APPDATA, or USERPROFILE is available".to_string()
        })
}

/// The file's text, or an empty document when there is no file yet.
///
/// A missing config file is not a problem to report: every setting in it has
/// a documented default, and a first run has nothing to read.
pub fn read_text(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

/// One root-level setting, with the line it was written on. A key inside a
/// table belongs to that table and is deliberately not this one.
pub fn root_setting<'a>(text: &'a str, key: &str) -> Result<Option<(usize, &'a str)>, String> {
    let mut found: Option<(usize, &str)> = None;
    let mut at_root = true;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            at_root = false;
            continue;
        }
        if !at_root {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        if found.is_some() {
            return Err(format!("{key} appears more than once"));
        }
        found = Some((index + 1, value.trim()));
    }
    Ok(found)
}

/// One root-level setting whose value is a quoted string, unquoted.
pub fn quoted_root_setting<'a>(
    text: &'a str,
    key: &str,
) -> Result<Option<(usize, &'a str)>, String> {
    root_setting(text, key)?
        .map(|(line, value)| {
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(|unquoted| (line, unquoted))
                .ok_or_else(|| {
                    format!(
                        "line {line} has invalid {key}: expected a quoted value in double quotes"
                    )
                })
        })
        .transpose()
}

/// The document with one root-level setting replaced and every other byte
/// left alone. A setting that is not there yet lands above the first table,
/// where it still belongs to the root.
pub fn with_root_setting(existing: &str, key: &str, rendered: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in existing.lines() {
        let significant = line.split('#').next().unwrap_or("").trim();
        if !replaced && significant.starts_with('[') {
            if !output.is_empty() && output.last().is_some_and(|line| !line.is_empty()) {
                output.push(String::new());
            }
            output.push(rendered.to_string());
            output.push(String::new());
            replaced = true;
        }
        let is_target = !significant.starts_with('[')
            && significant
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key)
            && !output.iter().any(|line| line.trim().starts_with('['));
        if is_target {
            if !replaced {
                output.push(rendered.to_string());
                replaced = true;
            }
        } else {
            output.push(line.to_string());
        }
    }
    if !replaced {
        if !output.is_empty() && output.last().is_some_and(|line| !line.is_empty()) {
            output.push(String::new());
        }
        output.push(rendered.to_string());
    }
    format!("{}\n", output.join("\n"))
}

/// The document with one root-level setting removed and every other byte
/// left alone.
pub fn without_root_setting(existing: &str, key: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut at_root = true;
    for line in existing.lines() {
        let significant = line.split('#').next().unwrap_or("").trim();
        if significant.starts_with('[') {
            at_root = false;
        }
        let is_target = at_root
            && significant
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key);
        if !is_target {
            output.push(line.to_string());
        }
    }
    while output.last().is_some_and(|line| line.trim().is_empty()) {
        output.pop();
    }
    if output.is_empty() {
        return String::new();
    }
    format!("{}\n", output.join("\n"))
}

/// Replaces the file with `text`, creating its directory, without ever
/// leaving a half-written settings file behind.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    crate::write_bundle_atomically(path, text)
        .map_err(|error| format!("could not replace {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_supports_windows_native_environment_fallbacks() {
        assert_eq!(
            user_config_path_from(
                Some(PathBuf::from("xdg")),
                Some(PathBuf::from("home")),
                Some(PathBuf::from("appdata")),
                Some(PathBuf::from("profile")),
            )
            .expect("XDG path"),
            PathBuf::from("xdg/kmp/config.toml")
        );
        assert_eq!(
            user_config_path_from(
                None,
                Some(PathBuf::from("home")),
                Some(PathBuf::from("appdata")),
                Some(PathBuf::from("profile")),
            )
            .expect("HOME path"),
            PathBuf::from("home/.config/kmp/config.toml")
        );
        assert_eq!(
            user_config_path_from(None, None, Some(PathBuf::from("appdata")), None)
                .expect("APPDATA path"),
            PathBuf::from("appdata/kmp/config.toml")
        );
        assert_eq!(
            user_config_path_from(None, None, None, Some(PathBuf::from("profile")))
                .expect("USERPROFILE path"),
            PathBuf::from("profile/.config/kmp/config.toml")
        );
        assert!(user_config_path_from(None, None, None, None).is_err());
    }

    #[test]
    fn a_root_setting_is_found_once_and_never_inside_a_table() {
        assert_eq!(
            root_setting("memory_store = \"/a\"\n", "memory_store").expect("one setting"),
            Some((1, "\"/a\""))
        );
        assert_eq!(
            root_setting("[future]\nmemory_store = \"/a\"\n", "memory_store").expect("subset"),
            None,
            "a key inside a table belongs to that table"
        );
        assert!(
            root_setting(
                "memory_store = \"/a\"\nmemory_store = \"/b\"\n",
                "memory_store"
            )
            .expect_err("a duplicate is ambiguous")
            .contains("appears more than once")
        );
        assert_eq!(
            root_setting("# memory_store = \"/a\"\n", "memory_store").expect("comment"),
            None
        );
    }

    #[test]
    fn a_quoted_setting_must_actually_be_quoted() {
        assert_eq!(
            quoted_root_setting("memory_store = \"/a\"\n", "memory_store").expect("quoted"),
            Some((1, "/a"))
        );
        let error = quoted_root_setting("memory_store = /a\n", "memory_store")
            .expect_err("an unquoted value is not TOML we accept");
        assert!(error.contains("line 1 has invalid memory_store"), "{error}");
        assert!(error.contains("quoted value"), "{error}");
    }

    #[test]
    fn replacing_one_setting_preserves_every_other_byte() {
        let replaced = with_root_setting(
            "future_setting = true\nmemory_routing = \"on_request\"\n",
            "memory_routing",
            "memory_routing = \"always\"",
        );
        assert!(replaced.contains("future_setting = true"));
        assert!(replaced.contains("memory_routing = \"always\""));
        assert_eq!(replaced.matches("memory_routing").count(), 1);

        let above_the_table = with_root_setting(
            "[future]\nmemory_routing = \"always\"\n",
            "memory_routing",
            "memory_routing = \"on_request\"",
        );
        assert_eq!(
            above_the_table,
            "memory_routing = \"on_request\"\n\n[future]\nmemory_routing = \"always\"\n"
        );
    }

    #[test]
    fn removing_one_setting_preserves_every_other_byte() {
        let cleared = without_root_setting(
            "future_setting = true\nmemory_store = \"/a\"\nmemory_routing = \"always\"\n",
            "memory_store",
        );
        assert_eq!(
            cleared,
            "future_setting = true\nmemory_routing = \"always\"\n"
        );
        assert_eq!(
            without_root_setting("[future]\nmemory_store = \"/a\"\n", "memory_store"),
            "[future]\nmemory_store = \"/a\"\n",
            "a key inside a table is not this setting"
        );
        assert_eq!(
            without_root_setting("memory_store = \"/a\"\n", "memory_store"),
            ""
        );
    }

    #[test]
    fn an_absent_file_reads_as_an_empty_document() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("kmp").join("config.toml");
        assert_eq!(read_text(&path).expect("absent file"), "");

        write_text(&path, "memory_store = \"/a\"\n").expect("write");
        assert_eq!(
            read_text(&path).expect("written file"),
            "memory_store = \"/a\"\n"
        );
    }
}
