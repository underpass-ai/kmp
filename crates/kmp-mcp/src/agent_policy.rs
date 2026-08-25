//! Agent-orchestration policy shared by every host path.
//!
//! The kernel remains deterministic and non-generative. This file stores the
//! small amount of persistent guidance an agent needs before it chooses a KMP
//! verb: temporal requests navigate time, and only semantic Ask may retry a
//! translated query. Stored evidence is never touched.

use std::path::{Path, PathBuf};

const KEY: &str = "ask_fallback_languages";
const DEFAULT_FALLBACK_LANGUAGE: &str = "en";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub ask_fallback_languages: Vec<String>,
    pub path: PathBuf,
    pub configured: bool,
}

impl AgentPolicy {
    pub fn source_label(&self) -> &'static str {
        if self.configured {
            "configured"
        } else {
            "default"
        }
    }
}

pub fn config_path() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(root).join("kmp").join("config.toml"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config").join("kmp").join("config.toml"))
        .ok_or_else(|| "neither XDG_CONFIG_HOME nor HOME is available".to_string())
}

pub fn load() -> Result<AgentPolicy, String> {
    let path = config_path()?;
    load_from(&path)
}

fn load_from(path: &Path) -> Result<AgentPolicy, String> {
    if !path.exists() {
        return Ok(AgentPolicy {
            ask_fallback_languages: vec![DEFAULT_FALLBACK_LANGUAGE.to_string()],
            path: path.to_path_buf(),
            configured: false,
        });
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let configured =
        parse_languages(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let is_configured = configured.is_some();
    Ok(AgentPolicy {
        ask_fallback_languages: configured
            .unwrap_or_else(|| vec![DEFAULT_FALLBACK_LANGUAGE.to_string()]),
        path: path.to_path_buf(),
        configured: is_configured,
    })
}

fn parse_languages(text: &str) -> Result<Option<Vec<String>>, String> {
    let mut parsed = None;
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
        if name.trim() != KEY {
            continue;
        }
        if parsed.is_some() {
            return Err(format!("{KEY} appears more than once"));
        }
        parsed = Some(
            parse_array(value.trim())
                .map_err(|error| format!("line {} has invalid {KEY}: {error}", index + 1))?,
        );
    }
    Ok(parsed)
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
    let path = config_path()?;
    let existing = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?
    } else {
        String::new()
    };
    let text = updated_config(&existing, languages);

    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    kmp_embedded::write_bundle_atomically(&path, &text)
        .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
    load_from(&path)
}

fn updated_config(existing: &str, languages: &[String]) -> String {
    let rendered = format!(
        "{KEY} = [{}]",
        languages
            .iter()
            .map(|language| format!("\"{language}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut output: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in existing.lines() {
        let significant = line.split('#').next().unwrap_or("").trim();
        if !replaced && significant.starts_with('[') {
            if !output.is_empty() && output.last().is_some_and(|line| !line.is_empty()) {
                output.push(String::new());
            }
            output.push(rendered.clone());
            output.push(String::new());
            replaced = true;
        }
        let is_target = !significant.starts_with('[')
            && significant
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == KEY)
            && !output.iter().any(|line| line.trim().starts_with('['));
        if is_target {
            if !replaced {
                output.push(rendered.clone());
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
        output.push(rendered);
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
        "KMP agent policy\n\nconfig: {}\nask fallback languages: {} ({})\n",
        policy.path.display(),
        languages,
        policy.source_label()
    )
}

pub fn mcp_instructions() -> String {
    match load() {
        Ok(policy) => mcp_instructions_for(&policy),
        Err(error) => format!(
            "KMP agent policy could not be loaded: {error}. Temporal intent still uses the time tools before kmp_ask. Do not perform cross-language Ask fallback until the policy is repaired. Stored evidence must never be translated or rewritten."
        ),
    }
}

fn mcp_instructions_for(policy: &AgentPolicy) -> String {
    let fallbacks = if policy.ask_fallback_languages.is_empty() {
        "none".to_string()
    } else {
        policy.ask_fallback_languages.join(", ")
    };
    format!(
        "Temporal intent has precedence over semantic Ask. For yesterday, today, since, before, after, during, explicit dates/timestamps, or release windows, resolve the user's timezone to an explicit half-open UTC interval [start, end), use kmp_goto/kmp_near/kmp_rewind/kmp_forward before kmp_ask, and continue pagination until the interval is complete or report the exact continuation state. Only a genuinely semantic kmp_ask may use cross-language fallback. Ask first in the user's language; if UNKNOWN or the evidence does not answer, translate only the query and retry each configured language at most once. Active Ask fallback languages: {fallbacks}. Answer in the user's language. Preserve evidence text, refs, relation why, and source metadata byte-for-byte. UNKNOWN remains a valid final answer."
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
    fn instructions_put_temporal_routing_before_semantic_fallback() {
        let policy = AgentPolicy {
            ask_fallback_languages: vec!["en".into()],
            path: PathBuf::from("/policy"),
            configured: true,
        };
        let instructions = mcp_instructions_for(&policy);

        assert!(
            instructions
                .find("Temporal intent")
                .expect("temporal clause")
                < instructions
                    .find("semantic kmp_ask")
                    .expect("semantic clause")
        );
        assert!(instructions.contains("half-open UTC interval [start, end)"));
        assert!(instructions.contains("Active Ask fallback languages: en"));
        assert!(instructions.contains("byte-for-byte"));
    }

    #[test]
    fn store_preserves_unrelated_configuration() {
        let replaced = updated_config(
            "future_setting = true\nask_fallback_languages = [\"fr\"]\n",
            &["en".to_string()],
        );

        assert!(replaced.contains("future_setting = true"));
        assert!(replaced.contains("ask_fallback_languages = [\"en\"]"));
        assert_eq!(replaced.matches(KEY).count(), 1);
    }

    #[test]
    fn root_policy_does_not_capture_or_enter_an_unrelated_table() {
        let existing = "[future]\nask_fallback_languages = [\"fr\"]\n";
        assert_eq!(parse_languages(existing).expect("valid TOML subset"), None);

        let replaced = updated_config(existing, &["en".to_string()]);
        assert_eq!(
            replaced,
            "ask_fallback_languages = [\"en\"]\n\n[future]\nask_fallback_languages = [\"fr\"]\n"
        );
        assert_eq!(
            parse_languages(&replaced).expect("root policy parses"),
            Some(vec!["en".to_string()])
        );
    }
}
