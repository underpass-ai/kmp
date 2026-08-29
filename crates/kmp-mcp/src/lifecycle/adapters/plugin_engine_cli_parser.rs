use std::path::PathBuf;

use crate::lifecycle::application::dto::plugin_engine_request_dto::PluginEngineRequestDto;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_engine_candidate::PluginEngineCandidate;
use crate::lifecycle::domain::plugin_engine_role::PluginEngineRole;
use crate::lifecycle::domain::plugin_root::PluginRoot;

pub struct PluginEngineCliParser;

impl PluginEngineCliParser {
    pub fn parse(arguments: &[&str]) -> Result<PluginEngineRequestDto, LifecycleError> {
        if arguments.first().copied() != Some("resolve-engine") {
            return Err(LifecycleError::InvalidCommand(
                "usage: kmp-mcp plugin resolve-engine --plugin-root DIR [--path-engine PATH] [--bundled-engine PATH]".to_string(),
            ));
        }
        let mut plugin_root = None;
        let mut candidates = Vec::new();
        let mut index = 1;
        while index < arguments.len() {
            let role = match arguments[index] {
                "--plugin-root" => {
                    index += 1;
                    plugin_root = Some(PathBuf::from(arguments.get(index).ok_or_else(|| {
                        LifecycleError::InvalidCommand(
                            "--plugin-root needs a directory".to_string(),
                        )
                    })?));
                    index += 1;
                    continue;
                }
                "--path-engine" => PluginEngineRole::Path,
                "--bundled-engine" => PluginEngineRole::Bundled,
                option => {
                    return Err(LifecycleError::InvalidCommand(format!(
                        "unknown plugin resolver option `{option}`"
                    )));
                }
            };
            index += 1;
            let path = PathBuf::from(arguments.get(index).ok_or_else(|| {
                LifecycleError::InvalidCommand(format!(
                    "{} needs an executable path",
                    arguments[index - 1]
                ))
            })?);
            if !path.is_absolute() {
                return Err(LifecycleError::UnsafePath(path));
            }
            candidates.push(PluginEngineCandidate::new(
                EngineExecutable::installed_at(path),
                role,
            ));
            index += 1;
        }
        let root = plugin_root.ok_or_else(|| {
            LifecycleError::InvalidCommand("plugin resolver needs --plugin-root DIR".to_string())
        })?;
        Ok(PluginEngineRequestDto {
            plugin_root: PluginRoot::new(root)?,
            candidates,
        })
    }
}
