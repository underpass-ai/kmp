use std::path::PathBuf;

use crate::application::dto::release_command_dto::ReleaseCommandDto;
use crate::domain::calendar_date::CalendarDate;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::plugin_package_kind::PluginPackageKind;
use crate::domain::plugin_repository::PluginRepository;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::domain::source_commit::SourceCommit;

pub struct ReleaseCommandMapper;

impl ReleaseCommandMapper {
    pub fn map(arguments: Vec<String>) -> Result<ReleaseCommandDto, ReleaseError> {
        let root = RepositoryRoot::discover()?;
        let usage = "usage: kmp-release version current|prepare [VERSION] [--root DIR]\n       kmp-release changelog prepare|check VERSION [--path FILE] [--date YYYY-MM-DD]\n       kmp-release readme sync [--source FILE] [--target FILE ...]\n       kmp-release guide sync VERSION --binary PATH [--root DIR]\n       kmp-release guide assets write|apply --binary PATH [--root DIR]\n       kmp-release plugin package --binary PATH [--output DIR] [--root DIR] [--release]\n       kmp-release mcpb package VERSION --input DIR [--output DIR] [--root DIR]\n       kmp-release mcpb stamp ARCHIVE [--server FILE] [--root DIR]\n       kmp-release marketplace verify VERSION [--root DIR] [--repository URL] [--expected-commit SHA] [--allow-unpublished-tag]\n       kmp-release candidate inputs [--github-output FILE]\n       kmp-release candidate assemble --version X.Y.Z --binaries DIR --plugins DIR --mcpb DIR --output DIR\n       kmp-release candidate verify --version X.Y.Z --directory DIR [--input-sha256 SHA256] [--run-id ID]";
        let command = arguments.first().map(String::as_str).unwrap_or_default();
        let action = arguments.get(1).map(String::as_str).unwrap_or_default();
        match (command, action) {
            ("version", "current") => {
                let selected_root = match arguments.as_slice() {
                    [_, _] => root,
                    [_, _, option, path] if option == "--root" => RepositoryRoot::from_path(path)?,
                    _ => {
                        return Err(ReleaseError::invalid(
                            "version current accepts only --root DIR",
                        ));
                    }
                };
                Ok(ReleaseCommandDto::CurrentVersion {
                    root: selected_root,
                })
            }
            ("version", "prepare") => {
                let version = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("version prepare needs a version"))
                    .and_then(|value| ReleaseVersion::parse(value.clone()))?;
                let selected_root = match arguments.as_slice() {
                    [_, _, _] => root,
                    [_, _, _, option, path] if option == "--root" => {
                        RepositoryRoot::from_path(path)?
                    }
                    _ => {
                        return Err(ReleaseError::invalid(
                            "version prepare accepts only VERSION and --root DIR",
                        ));
                    }
                };
                Ok(ReleaseCommandDto::PrepareVersion {
                    version,
                    root: selected_root,
                })
            }
            ("changelog", "prepare" | "check") => {
                let version = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid(usage))
                    .and_then(|value| ReleaseVersion::parse(value.clone()))?;
                let mut path = root.join("CHANGELOG.md");
                let mut date = None;
                let mut index = 3;
                while index < arguments.len() {
                    match arguments[index].as_str() {
                        "--path" => {
                            index += 1;
                            path = PathBuf::from(
                                arguments
                                    .get(index)
                                    .ok_or_else(|| ReleaseError::invalid("--path needs a file"))?,
                            );
                        }
                        "--date" if action == "prepare" => {
                            index += 1;
                            date = Some(CalendarDate::parse(arguments.get(index).ok_or_else(
                                || ReleaseError::invalid("--date needs YYYY-MM-DD"),
                            )?)?);
                        }
                        option => {
                            return Err(ReleaseError::invalid(format!(
                                "unknown changelog option `{option}`"
                            )));
                        }
                    }
                    index += 1;
                }
                if action == "prepare" {
                    Ok(ReleaseCommandDto::PrepareChangelog {
                        version,
                        date: match date {
                            Some(date) => date,
                            None => CalendarDate::today_utc()?,
                        },
                        path,
                    })
                } else {
                    Ok(ReleaseCommandDto::CheckChangelog { version, path })
                }
            }
            ("readme", "sync") => {
                let mut source = root.join("plugins/kmp/README.md");
                let mut targets = Vec::new();
                let mut index = 2;
                while index < arguments.len() {
                    match arguments[index].as_str() {
                        "--source" => {
                            index += 1;
                            source =
                                PathBuf::from(arguments.get(index).ok_or_else(|| {
                                    ReleaseError::invalid("--source needs a file")
                                })?);
                        }
                        "--target" => {
                            index += 1;
                            targets.push(PathBuf::from(
                                arguments.get(index).ok_or_else(|| {
                                    ReleaseError::invalid("--target needs a file")
                                })?,
                            ));
                        }
                        option => {
                            return Err(ReleaseError::invalid(format!(
                                "unknown readme option `{option}`"
                            )));
                        }
                    }
                    index += 1;
                }
                if targets.is_empty() {
                    targets = vec![
                        root.join("README.md"),
                        root.join("crates/kmp-mcp/README.md"),
                    ];
                }
                Ok(ReleaseCommandDto::SyncReadme { source, targets })
            }
            ("candidate", "inputs") => {
                let mut github_output = None;
                let mut index = 2;
                while index < arguments.len() {
                    match arguments[index].as_str() {
                        "--github-output" => {
                            index += 1;
                            github_output =
                                Some(PathBuf::from(arguments.get(index).ok_or_else(|| {
                                    ReleaseError::invalid("--github-output needs a file")
                                })?));
                        }
                        option => {
                            return Err(ReleaseError::invalid(format!(
                                "unknown candidate inputs option `{option}`"
                            )));
                        }
                    }
                    index += 1;
                }
                Ok(ReleaseCommandDto::CandidateInputs { github_output })
            }
            ("candidate", "assemble") => {
                let options = Self::named_options(&arguments[2..])?;
                Ok(ReleaseCommandDto::AssembleCandidate {
                    version: ReleaseVersion::parse(Self::required(&options, "--version")?)?,
                    binaries: PathBuf::from(Self::required(&options, "--binaries")?),
                    plugins: PathBuf::from(Self::required(&options, "--plugins")?),
                    mcpb: PathBuf::from(Self::required(&options, "--mcpb")?),
                    output: PathBuf::from(Self::required(&options, "--output")?),
                })
            }
            ("candidate", "verify") => {
                let options = Self::named_options(&arguments[2..])?;
                Ok(ReleaseCommandDto::VerifyCandidate {
                    version: ReleaseVersion::parse(Self::required(&options, "--version")?)?,
                    directory: PathBuf::from(Self::required(&options, "--directory")?),
                    input_digest: options
                        .get("--input-sha256")
                        .map(|value| CandidateInputDigest::parse(value.clone()))
                        .transpose()?,
                    run_id: options.get("--run-id").cloned(),
                })
            }
            ("guide", "sync") => {
                let version = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("guide sync needs a version"))
                    .and_then(|value| ReleaseVersion::parse(value.clone()))?;
                let options = Self::named_options(&arguments[3..])?;
                let selected_root = match options.get("--root") {
                    Some(path) => RepositoryRoot::from_path(path)?,
                    None => root,
                };
                Ok(ReleaseCommandDto::SyncGuide {
                    version,
                    root: selected_root,
                    binary: PathBuf::from(Self::required(&options, "--binary")?),
                })
            }
            ("guide", "assets") => {
                let asset_action = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("guide assets needs write or apply"))?;
                let options = Self::named_options(&arguments[3..])?;
                let selected_root = match options.get("--root") {
                    Some(path) => RepositoryRoot::from_path(path)?,
                    None => root,
                };
                let binary = PathBuf::from(Self::required(&options, "--binary")?);
                match asset_action.as_str() {
                    "write" => Ok(ReleaseCommandDto::WriteGuideAssets {
                        root: selected_root,
                        binary,
                    }),
                    "apply" => Ok(ReleaseCommandDto::ApplyGuideAssets {
                        root: selected_root,
                        binary,
                    }),
                    other => Err(ReleaseError::invalid(format!(
                        "unknown guide assets action `{other}`"
                    ))),
                }
            }
            ("plugin", "package") => {
                let mut selected_root = root;
                let mut binary = None;
                let mut output = None;
                let mut kind = PluginPackageKind::Development;
                let mut index = 2;
                while index < arguments.len() {
                    match arguments[index].as_str() {
                        "--root" => {
                            index += 1;
                            selected_root =
                                RepositoryRoot::from_path(arguments.get(index).ok_or_else(
                                    || ReleaseError::invalid("--root needs a directory"),
                                )?)?;
                        }
                        "--binary" => {
                            index += 1;
                            binary =
                                Some(PathBuf::from(arguments.get(index).ok_or_else(|| {
                                    ReleaseError::invalid("--binary needs a file")
                                })?));
                        }
                        "--output" => {
                            index += 1;
                            output =
                                Some(PathBuf::from(arguments.get(index).ok_or_else(|| {
                                    ReleaseError::invalid("--output needs a directory")
                                })?));
                        }
                        "--release" => kind = PluginPackageKind::Release,
                        option => {
                            return Err(ReleaseError::invalid(format!(
                                "unknown plugin package option `{option}`"
                            )));
                        }
                    }
                    index += 1;
                }
                Ok(ReleaseCommandDto::PackagePlugin {
                    binary: binary.ok_or_else(|| {
                        ReleaseError::invalid("plugin package needs --binary PATH")
                    })?,
                    output: output.unwrap_or_else(|| selected_root.join("dist/plugin")),
                    root: selected_root,
                    kind,
                })
            }
            ("mcpb", "package") => {
                let version = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("mcpb package needs a version"))
                    .and_then(|value| ReleaseVersion::parse(value.clone()))?;
                let options = Self::named_options(&arguments[3..])?;
                let selected_root = match options.get("--root") {
                    Some(path) => RepositoryRoot::from_path(path)?,
                    None => root,
                };
                let output = options
                    .get("--output")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| selected_root.join("dist/mcpb"));
                Ok(ReleaseCommandDto::PackageMcpb {
                    version,
                    root: selected_root,
                    input: PathBuf::from(Self::required(&options, "--input")?),
                    output,
                })
            }
            ("mcpb", "stamp") => {
                let archive = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("mcpb stamp needs an archive"))?;
                let options = Self::named_options(&arguments[3..])?;
                let selected_root = match options.get("--root") {
                    Some(path) => RepositoryRoot::from_path(path)?,
                    None => root,
                };
                let server_manifest = options
                    .get("--server")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| selected_root.join("server.json"));
                Ok(ReleaseCommandDto::StampServerMcpb {
                    root: selected_root,
                    archive: PathBuf::from(archive),
                    server_manifest,
                })
            }
            ("marketplace", "verify") => {
                let version = arguments
                    .get(2)
                    .ok_or_else(|| ReleaseError::invalid("marketplace verify needs a version"))
                    .and_then(|value| ReleaseVersion::parse(value.clone()))?;
                let mut selected_root = root;
                let mut repository = PluginRepository::URL.to_string();
                let mut expected_commit = None;
                let mut allow_unpublished_tag = false;
                let mut index = 3;
                while index < arguments.len() {
                    match arguments[index].as_str() {
                        "--root" => {
                            index += 1;
                            selected_root =
                                RepositoryRoot::from_path(arguments.get(index).ok_or_else(
                                    || ReleaseError::invalid("--root needs a directory"),
                                )?)?;
                        }
                        "--repository" => {
                            index += 1;
                            repository = arguments
                                .get(index)
                                .ok_or_else(|| ReleaseError::invalid("--repository needs a URL"))?
                                .clone();
                        }
                        "--expected-commit" => {
                            index += 1;
                            expected_commit =
                                Some(SourceCommit::parse(arguments.get(index).ok_or_else(
                                    || ReleaseError::invalid("--expected-commit needs a SHA"),
                                )?)?);
                        }
                        "--allow-unpublished-tag" => allow_unpublished_tag = true,
                        option => {
                            return Err(ReleaseError::invalid(format!(
                                "unknown marketplace option `{option}`"
                            )));
                        }
                    }
                    index += 1;
                }
                if allow_unpublished_tag && expected_commit.is_none() {
                    return Err(ReleaseError::invalid(
                        "--allow-unpublished-tag requires --expected-commit",
                    ));
                }
                Ok(ReleaseCommandDto::VerifyMarketplace {
                    version,
                    root: selected_root,
                    repository,
                    expected_commit,
                    allow_unpublished_tag,
                })
            }
            _ => Err(ReleaseError::invalid(usage)),
        }
    }

    fn named_options(
        arguments: &[String],
    ) -> Result<std::collections::BTreeMap<String, String>, ReleaseError> {
        let mut options = std::collections::BTreeMap::new();
        let mut chunks = arguments.chunks_exact(2);
        for pair in &mut chunks {
            if !pair[0].starts_with("--") {
                return Err(ReleaseError::invalid(format!(
                    "unexpected argument `{}`",
                    pair[0]
                )));
            }
            if options.insert(pair[0].clone(), pair[1].clone()).is_some() {
                return Err(ReleaseError::invalid(format!(
                    "duplicate option `{}`",
                    pair[0]
                )));
            }
        }
        if let Some(argument) = chunks.remainder().first() {
            return Err(ReleaseError::invalid(format!(
                "option `{argument}` needs a value"
            )));
        }
        Ok(options)
    }

    fn required(
        options: &std::collections::BTreeMap<String, String>,
        name: &str,
    ) -> Result<String, ReleaseError> {
        options
            .get(name)
            .cloned()
            .ok_or_else(|| ReleaseError::invalid(format!("candidate command requires {name}")))
    }
}
