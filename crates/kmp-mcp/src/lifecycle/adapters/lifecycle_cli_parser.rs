use std::path::PathBuf;

use crate::lifecycle::application::dto::lifecycle_command_dto::LifecycleCommandDto;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Inbound CLI adapter. Strings and environment variables stop here.
#[derive(Clone, Copy, Debug, Default)]
pub struct LifecycleCliParser;

impl LifecycleCliParser {
    pub fn parse(arguments: &[&str]) -> Result<LifecycleCommandDto, LifecycleError> {
        let mut hosts = Vec::new();
        let mut version = None;
        let mut install_dir = None;
        let mut dry_run = false;
        let mut lexical_bridge = None;
        let mut decline_bridge = false;
        let mut position = 0;

        while position < arguments.len() {
            match arguments[position] {
                "--claude" => hosts.push("claude".to_string()),
                "--codex" => hosts.push("codex".to_string()),
                "--dry-run" => dry_run = true,
                "--no-lexical-bridge" => decline_bridge = true,
                "--lexical-bridge" => {
                    lexical_bridge = Some(PathBuf::from(Self::value(
                        arguments,
                        position,
                        "--lexical-bridge",
                    )?));
                    position += 1;
                }
                "--version" => {
                    version = Some(Self::value(arguments, position, "--version")?.to_string());
                    position += 1;
                }
                "--dir" | "--engine-dir" => {
                    install_dir = Some(PathBuf::from(Self::value(
                        arguments,
                        position,
                        arguments[position],
                    )?));
                    position += 1;
                }
                "--standalone" => {
                    return Err(LifecycleError::InvalidCommand(
                        "--standalone was retired; install KMP through the native Codex plugin manager"
                            .to_string(),
                    ));
                }
                argument => {
                    return Err(LifecycleError::InvalidCommand(format!(
                        "unknown lifecycle option `{argument}`"
                    )));
                }
            }
            position += 1;
        }

        if decline_bridge && lexical_bridge.is_some() {
            return Err(LifecycleError::InvalidCommand(
                "--lexical-bridge and --no-lexical-bridge ask for opposite things".to_string(),
            ));
        }
        Ok(LifecycleCommandDto {
            hosts,
            version,
            install_dir: install_dir.unwrap_or_else(Self::default_install_dir),
            dry_run,
            lexical_bridge,
            decline_bridge,
            bridge_dir: Self::default_bridge_dir(),
        })
    }

    fn value<'a>(
        arguments: &'a [&str],
        position: usize,
        option: &str,
    ) -> Result<&'a str, LifecycleError> {
        arguments
            .get(position + 1)
            .copied()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| LifecycleError::InvalidCommand(format!("{option} requires a value")))
    }

    /// Where the machine's one lexical-bridge table goes: beside the
    /// stores, not inside one, because a store is selected per working
    /// directory and the table is the same several megabytes for all of them.
    fn default_bridge_dir() -> Option<PathBuf> {
        kmp_embedded::user_data_home().map(|home| home.join("kmp"))
    }

    fn default_install_dir() -> PathBuf {
        std::env::var_os("KMP_INSTALL_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local").join("bin"))
            })
            .unwrap_or_else(|| PathBuf::from(".local/bin"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_boundary_values_without_leaking_flags_into_the_domain() {
        let dto = LifecycleCliParser::parse(&[
            "--codex",
            "--version",
            "v0.5.1",
            "--engine-dir",
            "/tmp/kmp-bin",
            "--dry-run",
        ])
        .expect("command");

        assert_eq!(dto.hosts, vec!["codex"]);
        assert_eq!(dto.version.as_deref(), Some("v0.5.1"));
        assert_eq!(dto.install_dir, PathBuf::from("/tmp/kmp-bin"));
        assert!(dto.dry_run);
        assert_eq!(dto.lexical_bridge, None);
        assert!(!dto.decline_bridge);
    }

    #[test]
    fn a_table_the_operator_built_is_carried_as_a_path() {
        let dto =
            LifecycleCliParser::parse(&["--lexical-bridge", "/tmp/es-en.kmpb"]).expect("command");

        assert_eq!(dto.lexical_bridge, Some(PathBuf::from("/tmp/es-en.kmpb")));
        assert!(!dto.decline_bridge);
    }

    #[test]
    fn declining_the_table_is_its_own_flag() {
        let dto = LifecycleCliParser::parse(&["--no-lexical-bridge"]).expect("command");

        assert!(dto.decline_bridge);
        assert_eq!(dto.lexical_bridge, None);
    }

    #[test]
    fn asking_for_a_table_and_for_none_is_refused_rather_than_ranked() {
        assert!(
            LifecycleCliParser::parse(&[
                "--lexical-bridge",
                "/tmp/es-en.kmpb",
                "--no-lexical-bridge",
            ])
            .is_err()
        );
    }

    #[test]
    fn standalone_is_rejected_at_the_boundary() {
        assert!(LifecycleCliParser::parse(&["--standalone"]).is_err());
    }
}
