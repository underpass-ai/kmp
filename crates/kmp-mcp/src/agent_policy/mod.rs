//! Agent-orchestration policy shared by every host path.
//!
//! The kernel remains deterministic and non-generative. This module stores the
//! small amount of persistent guidance an agent needs before it chooses a KMP
//! verb: whether memory is entered at all. Everything else the agent is told
//! — that temporal requests navigate time, that a semantic question is asked
//! in the kernel's search language with the user's words as `asked_as` — is
//! fixed by the kernel and not configured. Stored evidence is never touched.

pub mod instructions;
pub mod memory_routing;

pub use instructions::mcp_instructions;
pub use memory_routing::MemoryRouting;

use std::path::{Path, PathBuf};

/// The setting a previous release configured and this one no longer reads.
/// A file that still carries it is read without it, and the doctor says so.
const RETIRED_FALLBACK_KEY: &str = "ask_fallback_languages";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub memory_routing: MemoryRouting,
    pub path: PathBuf,
    pub routing_configured: bool,
    /// Whether the file still carries `ask_fallback_languages`, which nothing
    /// reads any more.
    pub retired_fallback_setting: bool,
}

impl AgentPolicy {
    pub fn routing_source_label(&self) -> &'static str {
        if self.routing_configured {
            "configured"
        } else {
            "default"
        }
    }

    /// What to tell an operator whose file still carries the retired
    /// setting, or nothing when it does not.
    pub fn retired_setting_notice(&self) -> Option<String> {
        self.retired_fallback_setting.then(|| {
            format!(
                "{RETIRED_FALLBACK_KEY} is no longer read: a semantic question is asked in \
                 English with the user's words as asked_as; remove the line from {}",
                self.path.display()
            )
        })
    }
}

pub fn config_path() -> Result<PathBuf, String> {
    let path_from = |name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    config_path_from(
        path_from("XDG_CONFIG_HOME"),
        path_from("HOME"),
        path_from("APPDATA"),
        path_from("USERPROFILE"),
    )
}

fn config_path_from(
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

pub fn load() -> Result<AgentPolicy, String> {
    let path = config_path()?;
    load_from(&path)
}

fn load_from(path: &Path) -> Result<AgentPolicy, String> {
    if !path.exists() {
        return Ok(default_policy(path));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    parse_policy(&text, path).map_err(|error| format!("{}: {error}", path.display()))
}

fn default_policy(path: &Path) -> AgentPolicy {
    AgentPolicy {
        memory_routing: MemoryRouting::default(),
        path: path.to_path_buf(),
        routing_configured: false,
        retired_fallback_setting: false,
    }
}

fn parse_policy(text: &str, path: &Path) -> Result<AgentPolicy, String> {
    let routing = parse_memory_routing(text)?;
    // The retired key is looked for, never parsed: whatever it says, it is
    // not a reason to refuse a file whose only other setting is valid.
    let retired_fallback_setting = root_setting(text, RETIRED_FALLBACK_KEY)
        .map(|found| found.is_some())
        .unwrap_or(true);
    Ok(AgentPolicy {
        routing_configured: routing.is_some(),
        memory_routing: routing.unwrap_or_default(),
        path: path.to_path_buf(),
        retired_fallback_setting,
    })
}

/// One root-level setting, with the line it was written on. A key inside a
/// table belongs to that table and is deliberately not this one.
fn root_setting<'a>(text: &'a str, key: &str) -> Result<Option<(usize, &'a str)>, String> {
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

fn parse_memory_routing(text: &str) -> Result<Option<MemoryRouting>, String> {
    let routing_key = memory_routing::KEY;
    root_setting(text, routing_key)?
        .map(|(line, value)| {
            let quoted = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(|| {
                    format!(
                        "line {line} has invalid {routing_key}: expected a quoted mode such as \"on_request\""
                    )
                })?;
            MemoryRouting::parse(quoted)
                .map_err(|error| format!("line {line} has invalid {routing_key}: {error}"))
        })
        .transpose()
}

pub fn store_memory_routing(routing: MemoryRouting) -> Result<AgentPolicy, String> {
    let rendered = format!("{} = \"{}\"", memory_routing::KEY, routing.config_value());
    store_setting(memory_routing::KEY, &rendered)
}

fn store_setting(key: &str, rendered: &str) -> Result<AgentPolicy, String> {
    let path = config_path()?;
    let existing = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?
    } else {
        String::new()
    };
    let text = updated_config(&existing, key, rendered);

    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    kmp_embedded::write_bundle_atomically(&path, &text)
        .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
    load_from(&path)
}

/// Replace one root-level setting and leave every other byte of the file
/// alone. A setting that is not there yet lands above the first table, where
/// it still belongs to the root.
fn updated_config(existing: &str, key: &str, rendered: &str) -> String {
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

pub fn display(policy: &AgentPolicy) -> String {
    let mut rendered = format!(
        "KMP agent policy\n\nconfig: {}\nmemory routing: {} ({})\n",
        policy.path.display(),
        policy.memory_routing.label(),
        policy.routing_source_label(),
    );
    if let Some(notice) = policy.retired_setting_notice() {
        rendered.push_str("note: ");
        rendered.push_str(&notice);
        rendered.push('\n');
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_supports_windows_native_environment_fallbacks() {
        assert_eq!(
            config_path_from(
                Some(PathBuf::from("xdg")),
                Some(PathBuf::from("home")),
                Some(PathBuf::from("appdata")),
                Some(PathBuf::from("profile")),
            )
            .expect("XDG path"),
            PathBuf::from("xdg/kmp/config.toml")
        );
        assert_eq!(
            config_path_from(
                None,
                Some(PathBuf::from("home")),
                Some(PathBuf::from("appdata")),
                Some(PathBuf::from("profile")),
            )
            .expect("HOME path"),
            PathBuf::from("home/.config/kmp/config.toml")
        );
        assert_eq!(
            config_path_from(None, None, Some(PathBuf::from("appdata")), None)
                .expect("APPDATA path"),
            PathBuf::from("appdata/kmp/config.toml")
        );
        assert_eq!(
            config_path_from(None, None, None, Some(PathBuf::from("profile")))
                .expect("USERPROFILE path"),
            PathBuf::from("profile/.config/kmp/config.toml")
        );
    }

    #[test]
    fn store_preserves_unrelated_configuration() {
        let replaced = updated_config(
            "future_setting = true\nmemory_routing = \"on_request\"\n",
            memory_routing::KEY,
            "memory_routing = \"always\"",
        );

        assert!(replaced.contains("future_setting = true"));
        assert!(replaced.contains("memory_routing = \"always\""));
        assert_eq!(replaced.matches(memory_routing::KEY).count(), 1);
    }

    #[test]
    fn root_policy_does_not_capture_or_enter_an_unrelated_table() {
        let existing = "[future]\nmemory_routing = \"always\"\n";
        assert_eq!(
            parse_memory_routing(existing).expect("valid TOML subset"),
            None
        );

        let replaced = updated_config(existing, memory_routing::KEY, "memory_routing = \"always\"");
        assert_eq!(
            replaced,
            "memory_routing = \"always\"\n\n[future]\nmemory_routing = \"always\"\n"
        );
        assert_eq!(
            parse_memory_routing(&replaced).expect("root policy parses"),
            Some(MemoryRouting::Always)
        );
    }

    #[test]
    fn memory_routing_defaults_to_on_request_and_reports_its_source() {
        let default = parse_policy("", Path::new("/policy")).expect("empty policy is valid");
        assert_eq!(default.memory_routing, MemoryRouting::OnRequest);
        assert_eq!(default.routing_source_label(), "default");
        assert_eq!(default, default_policy(Path::new("/policy")));

        let configured = parse_policy("memory_routing = \"always\"\n", Path::new("/policy"))
            .expect("valid policy");
        assert_eq!(configured.memory_routing, MemoryRouting::Always);
        assert_eq!(configured.routing_source_label(), "configured");
        assert!(!configured.retired_fallback_setting);
    }

    /// A file written by a release that still configured fallback languages
    /// is read without them: the setting is noticed, never parsed, and never
    /// a reason to refuse the file.
    #[test]
    fn a_retired_fallback_setting_is_noticed_and_otherwise_ignored() {
        let policy = parse_policy(
            "ask_fallback_languages = [\"en\", \"fr\"]\nmemory_routing = \"always\"\n",
            Path::new("/policy"),
        )
        .expect("the retired setting does not invalidate the file");

        assert_eq!(policy.memory_routing, MemoryRouting::Always);
        assert!(policy.retired_fallback_setting);
        let notice = policy.retired_setting_notice().expect("notice");
        assert!(notice.contains("no longer read"), "{notice}");
        assert!(notice.contains("asked_as"), "{notice}");
        assert!(notice.contains("/policy"), "{notice}");

        // Even a value the old parser would have refused is only noticed.
        let malformed = parse_policy("ask_fallback_languages = en\n", Path::new("/policy"))
            .expect("a retired setting is not parsed");
        assert!(malformed.retired_fallback_setting);
        assert_eq!(malformed.memory_routing, MemoryRouting::OnRequest);

        let clean = parse_policy("", Path::new("/policy")).expect("empty");
        assert_eq!(clean.retired_setting_notice(), None);
    }

    #[test]
    fn rejects_a_malformed_or_duplicated_memory_routing() {
        let unquoted = parse_memory_routing("memory_routing = always\n")
            .expect_err("an unquoted mode is not TOML we accept");
        assert!(unquoted.contains("line 1 has invalid memory_routing"));
        assert!(unquoted.contains("quoted mode"));

        let unsupported = parse_memory_routing("memory_routing = \"sometimes\"\n")
            .expect_err("an unsupported mode must fail");
        assert!(unsupported.contains("is not a memory routing mode"));

        assert!(
            parse_memory_routing("memory_routing = \"always\"\nmemory_routing = \"on_request\"\n")
                .expect_err("a duplicate must fail")
                .contains("appears more than once")
        );
        assert_eq!(
            parse_memory_routing("[future]\nmemory_routing = \"always\"\n")
                .expect("valid TOML subset"),
            None,
            "a key inside a table belongs to that table"
        );
    }

    #[test]
    fn writing_the_routing_leaves_a_retired_line_where_it_is() {
        let with_retired = updated_config(
            "ask_fallback_languages = [\"en\", \"fr\"]\n",
            memory_routing::KEY,
            "memory_routing = \"always\"",
        );
        let policy = parse_policy(&with_retired, Path::new("/policy")).expect("valid policy");
        assert_eq!(policy.memory_routing, MemoryRouting::Always);
        assert!(policy.retired_fallback_setting);

        let then_back = updated_config(
            &with_retired,
            memory_routing::KEY,
            "memory_routing = \"on_request\"",
        );
        assert_eq!(then_back.matches(memory_routing::KEY).count(), 1);
        assert!(then_back.contains("ask_fallback_languages"));
    }

    #[test]
    fn display_names_the_routing_and_the_retired_setting() {
        let rendered = display(&AgentPolicy {
            memory_routing: MemoryRouting::Always,
            path: PathBuf::from("/policy"),
            routing_configured: true,
            retired_fallback_setting: true,
        });

        assert!(rendered.contains("memory routing: always (configured)"));
        assert!(rendered.contains("note: ask_fallback_languages is no longer read"));
        assert!(!rendered.contains("ask fallback languages:"));
        let default = display(&default_policy(Path::new("/policy")));
        assert!(default.contains("memory routing: on request (default)"));
        assert!(!default.contains("note:"));
    }
}
