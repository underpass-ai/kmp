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
        let mut position = 0;

        while position < arguments.len() {
            match arguments[position] {
                "--claude" => hosts.push("claude".to_string()),
                "--codex" => hosts.push("codex".to_string()),
                "--dry-run" => dry_run = true,
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

        Ok(LifecycleCommandDto {
            hosts,
            version,
            install_dir: install_dir.unwrap_or_else(Self::default_install_dir),
            dry_run,
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
    }

    #[test]
    fn standalone_is_rejected_at_the_boundary() {
        assert!(LifecycleCliParser::parse(&["--standalone"]).is_err());
    }
}
