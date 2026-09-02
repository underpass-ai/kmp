//! Agent-orchestration policy shared by every host path.
//!
//! The kernel remains deterministic and non-generative. This module stores the
//! small amount of persistent guidance an agent needs before it chooses a KMP
//! verb: whether memory is entered at all, that temporal requests navigate
//! time, and that only semantic Ask may retry a translated query. Stored
//! evidence is never touched.

pub mod instructions;
pub mod memory_routing;

pub use instructions::mcp_instructions;
pub use memory_routing::MemoryRouting;

use std::path::{Path, PathBuf};

const KEY: &str = "ask_fallback_languages";
const DEFAULT_FALLBACK_LANGUAGE: &str = "en";
const UNSEGMENTED_FALLBACK_LANGUAGES: [&str; 3] = ["zh", "ja", "th"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub ask_fallback_languages: Vec<String>,
    pub memory_routing: MemoryRouting,
    pub path: PathBuf,
    pub configured: bool,
    pub routing_configured: bool,
}

impl AgentPolicy {
    pub fn source_label(&self) -> &'static str {
        source_label(self.configured)
    }

    pub fn routing_source_label(&self) -> &'static str {
        source_label(self.routing_configured)
    }
}

fn source_label(configured: bool) -> &'static str {
    if configured { "configured" } else { "default" }
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
        ask_fallback_languages: vec![DEFAULT_FALLBACK_LANGUAGE.to_string()],
        memory_routing: MemoryRouting::default(),
        path: path.to_path_buf(),
        configured: false,
        routing_configured: false,
    }
}

fn parse_policy(text: &str, path: &Path) -> Result<AgentPolicy, String> {
    let languages = parse_languages(text)?;
    let routing = parse_memory_routing(text)?;
    Ok(AgentPolicy {
        configured: languages.is_some(),
        routing_configured: routing.is_some(),
        ask_fallback_languages: languages
            .unwrap_or_else(|| vec![DEFAULT_FALLBACK_LANGUAGE.to_string()]),
        memory_routing: routing.unwrap_or_default(),
        path: path.to_path_buf(),
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

fn parse_languages(text: &str) -> Result<Option<Vec<String>>, String> {
    root_setting(text, KEY)?
        .map(|(line, value)| {
            parse_array(value).map_err(|error| format!("line {line} has invalid {KEY}: {error}"))
        })
        .transpose()
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

fn parse_array(value: &str) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| "expected a TOML string array such as [\"en\"]".to_string())?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    let mut languages = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        let language = item
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| format!("{item:?} is not a quoted language tag"))?;
        let language = normalize_tag(language)?;
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    Ok(languages)
}

fn normalize_tag(value: &str) -> Result<String, String> {
    let tag = value.trim().to_ascii_lowercase();
    if tag.is_empty()
        || tag.len() > 35
        || tag.starts_with('-')
        || tag.ends_with('-')
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("{value:?} is not a language tag"));
    }
    let primary = tag.split('-').next().unwrap_or_default();
    if UNSEGMENTED_FALLBACK_LANGUAGES.contains(&primary) {
        return Err(format!(
            "{value:?} is not a supported Ask fallback language yet: Ask cannot retrieve by \
             word in Chinese, Japanese, or Thai text without segmentation; stored memory \
             remains byte-exact"
        ));
    }
    Ok(tag)
}

pub fn parse_cli_languages(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.is_empty() {
        return Ok(Vec::new());
    }
    let mut languages = Vec::new();
    for value in value.split(',') {
        let language = normalize_tag(value)?;
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    Ok(languages)
}

pub fn store(languages: &[String]) -> Result<AgentPolicy, String> {
    let rendered = format!(
        "{KEY} = [{}]",
        languages
            .iter()
            .map(|language| format!("\"{language}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    store_setting(KEY, &rendered)
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
    let languages = if policy.ask_fallback_languages.is_empty() {
        "none".to_string()
    } else {
        policy.ask_fallback_languages.join(", ")
    };
    format!(
        "KMP agent policy\n\nconfig: {}\nmemory routing: {} ({})\nask fallback languages: {} ({})\n",
        policy.path.display(),
        policy.memory_routing.label(),
        policy.routing_source_label(),
        languages,
        policy.source_label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalizes_the_configured_list() {
        let parsed = parse_languages(
            "# agent policy\nask_fallback_languages = [\"EN\", \"es-ES\", \"en\"]\n",
        )
        .expect("valid policy")
        .expect("configured list");

        assert_eq!(parsed, ["en", "es-es"]);
    }

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
    fn rejects_duplicate_or_malformed_policy() {
        assert!(parse_languages("ask_fallback_languages = en\n").is_err());
        assert!(
            parse_languages(
                "ask_fallback_languages = [\"en\"]\nask_fallback_languages = [\"fr\"]\n"
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unsegmented_ask_fallback_languages_and_their_variants() {
        for tag in ["zh", "ZH-Hans", "ja-JP", "th-TH"] {
            let error = parse_cli_languages(tag).expect_err("unsupported fallback must fail");
            assert!(error.contains("not a supported Ask fallback language yet"));
            assert!(error.contains("without segmentation"));
        }
        assert!(
            parse_languages("ask_fallback_languages = [\"en\", \"zh-Hant\"]\n")
                .expect_err("an unsupported configured fallback must fail")
                .contains("without segmentation")
        );
        assert_eq!(
            parse_cli_languages("es,fr,ar").expect("space-delimited scripts remain supported"),
            ["es", "fr", "ar"]
        );
    }

    #[test]
    fn store_preserves_unrelated_configuration() {
        let replaced = updated_config(
            "future_setting = true\nask_fallback_languages = [\"fr\"]\n",
            KEY,
            "ask_fallback_languages = [\"en\"]",
        );

        assert!(replaced.contains("future_setting = true"));
        assert!(replaced.contains("ask_fallback_languages = [\"en\"]"));
        assert_eq!(replaced.matches(KEY).count(), 1);
    }

    #[test]
    fn root_policy_does_not_capture_or_enter_an_unrelated_table() {
        let existing = "[future]\nask_fallback_languages = [\"fr\"]\n";
        assert_eq!(parse_languages(existing).expect("valid TOML subset"), None);

        let replaced = updated_config(existing, KEY, "ask_fallback_languages = [\"en\"]");
        assert_eq!(
            replaced,
            "ask_fallback_languages = [\"en\"]\n\n[future]\nask_fallback_languages = [\"fr\"]\n"
        );
        assert_eq!(
            parse_languages(&replaced).expect("root policy parses"),
            Some(vec!["en".to_string()])
        );
    }

    #[test]
    fn memory_routing_defaults_to_on_request_and_reports_its_source() {
        let default = parse_policy("", Path::new("/policy")).expect("empty policy is valid");
        assert_eq!(default.memory_routing, MemoryRouting::OnRequest);
        assert_eq!(default.routing_source_label(), "default");
        assert_eq!(default, default_policy(Path::new("/policy")));

        let configured = parse_policy(
            "memory_routing = \"always\"\nask_fallback_languages = [\"fr\"]\n",
            Path::new("/policy"),
        )
        .expect("valid policy");
        assert_eq!(configured.memory_routing, MemoryRouting::Always);
        assert_eq!(configured.routing_source_label(), "configured");
        assert_eq!(configured.source_label(), "configured");

        // Opting into always-on routing must not silently configure anything
        // else, and configuring a fallback list must not opt into routing.
        let routing_only =
            parse_policy("memory_routing = \"always\"\n", Path::new("/policy")).expect("valid");
        assert_eq!(routing_only.ask_fallback_languages, ["en"]);
        assert_eq!(routing_only.source_label(), "default");
        let languages_only =
            parse_policy("ask_fallback_languages = [\"fr\"]\n", Path::new("/policy"))
                .expect("valid");
        assert_eq!(languages_only.memory_routing, MemoryRouting::OnRequest);
        assert_eq!(languages_only.routing_source_label(), "default");
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
    fn each_setting_is_written_without_disturbing_the_other() {
        let with_languages = updated_config(
            "ask_fallback_languages = [\"en\", \"fr\"]\n",
            memory_routing::KEY,
            "memory_routing = \"always\"",
        );
        let policy = parse_policy(&with_languages, Path::new("/policy")).expect("valid policy");
        assert_eq!(policy.ask_fallback_languages, ["en", "fr"]);
        assert_eq!(policy.memory_routing, MemoryRouting::Always);

        let then_back = updated_config(
            &with_languages,
            memory_routing::KEY,
            "memory_routing = \"on_request\"",
        );
        assert_eq!(then_back.matches(memory_routing::KEY).count(), 1);
        let policy = parse_policy(&then_back, Path::new("/policy")).expect("valid policy");
        assert_eq!(policy.memory_routing, MemoryRouting::OnRequest);
        assert_eq!(policy.ask_fallback_languages, ["en", "fr"]);
    }

    #[test]
    fn display_names_both_settings_and_where_they_came_from() {
        let rendered = display(&AgentPolicy {
            ask_fallback_languages: Vec::new(),
            memory_routing: MemoryRouting::Always,
            path: PathBuf::from("/policy"),
            configured: true,
            routing_configured: true,
        });

        assert!(rendered.contains("memory routing: always (configured)"));
        assert!(rendered.contains("ask fallback languages: none (configured)"));
        assert!(
            display(&default_policy(Path::new("/policy")))
                .contains("memory routing: on request (default)")
        );
    }
}
