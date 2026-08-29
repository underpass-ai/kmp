use crate::application::dto::release_command_dto::ReleaseCommandDto;
use crate::application::use_cases::apply_guide_assets::ApplyGuideAssets;
use crate::application::use_cases::assemble_candidate::AssembleCandidate;
use crate::application::use_cases::calculate_candidate_inputs::CalculateCandidateInputs;
use crate::application::use_cases::check_changelog::CheckChangelog;
use crate::application::use_cases::package_mcpb::PackageMcpb;
use crate::application::use_cases::package_plugin::PackagePlugin;
use crate::application::use_cases::prepare_changelog::PrepareChangelog;
use crate::application::use_cases::prepare_release_version::PrepareReleaseVersion;
use crate::application::use_cases::read_workspace_version::ReadWorkspaceVersion;
use crate::application::use_cases::stamp_server_mcpb::StampServerMcpb;
use crate::application::use_cases::sync_public_readme::SyncPublicReadme;
use crate::application::use_cases::verify_candidate::VerifyCandidate;
use crate::application::use_cases::verify_marketplace::VerifyMarketplace;
use crate::application::use_cases::write_guide_assets::WriteGuideAssets;
use crate::domain::release_error::ReleaseError;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::guide_engine_factory::GuideEngineFactory;
use crate::ports::marketplace_repository::MarketplaceRepository;
use crate::ports::plugin_archive_writer::PluginArchiveWriter;
use crate::ports::release_archive_writer::ReleaseArchiveWriter;
use crate::ports::release_binary_version_reader::ReleaseBinaryVersionReader;
use crate::ports::release_environment::ReleaseEnvironment;
use crate::ports::release_file_system::ReleaseFileSystem;
use crate::ports::release_repository::ReleaseRepository;

pub struct ReleaseApplication<F, R, E, G, A, P> {
    file_system: F,
    repository: R,
    environment: E,
    guide_engines: G,
    archives: A,
    plugin_archives: P,
}

impl<F, R, E, G, A, P> ReleaseApplication<F, R, E, G, A, P>
where
    F: ReleaseFileSystem + CandidateFileSystem,
    R: ReleaseRepository + MarketplaceRepository,
    E: ReleaseEnvironment,
    G: GuideEngineFactory + ReleaseBinaryVersionReader,
    A: ReleaseArchiveWriter,
    P: PluginArchiveWriter,
{
    pub fn new(
        file_system: F,
        repository: R,
        environment: E,
        guide_engines: G,
        archives: A,
        plugin_archives: P,
    ) -> Self {
        Self {
            file_system,
            repository,
            environment,
            guide_engines,
            archives,
            plugin_archives,
        }
    }

    pub fn execute(&self, command: ReleaseCommandDto) -> Result<String, ReleaseError> {
        match command {
            ReleaseCommandDto::PrepareChangelog {
                version,
                date,
                path,
            } => {
                let changed =
                    PrepareChangelog::new(&self.file_system).execute(&path, &version, &date)?;
                Ok(format!(
                    "changelog: {} [{version}] in {}",
                    if changed {
                        "prepared"
                    } else {
                        "already prepared"
                    },
                    path.display()
                ))
            }
            ReleaseCommandDto::CheckChangelog { version, path } => {
                CheckChangelog::new(&self.file_system).execute(&path, &version)?;
                Ok(format!(
                    "changelog: verified [{version}] in {}",
                    path.display()
                ))
            }
            ReleaseCommandDto::SyncReadme { source, targets } => {
                let changed =
                    SyncPublicReadme::new(&self.file_system).execute(&source, &targets)?;
                Ok(if changed == 0 {
                    "public README sync: already current".to_string()
                } else {
                    format!("public README sync: updated {changed} surface(s)")
                })
            }
            ReleaseCommandDto::CandidateInputs { github_output } => {
                let root = crate::domain::repository_root::RepositoryRoot::discover()?;
                let digest = CalculateCandidateInputs::new(&self.file_system, &self.repository)
                    .execute(&root)?;
                if let Some(path) = github_output {
                    let mut existing = if path.exists() {
                        self.file_system.read_text(&path)?
                    } else {
                        String::new()
                    };
                    existing.push_str(&format!("input_sha256={digest}\n"));
                    self.file_system.write_text(&path, &existing)?;
                }
                Ok(digest.to_string())
            }
            ReleaseCommandDto::AssembleCandidate {
                version,
                binaries,
                plugins,
                mcpb,
                output,
            } => {
                let root = crate::domain::repository_root::RepositoryRoot::discover()?;
                let manifest =
                    AssembleCandidate::new(&self.file_system, &self.repository, &self.environment)
                        .execute(&root, &version, &[binaries, plugins, mcpb], &output)?;
                Ok(format!(
                    "release candidate assembled: 20 files, inputs {}, run {}",
                    manifest.input_sha256, manifest.run_id
                ))
            }
            ReleaseCommandDto::VerifyCandidate {
                version,
                directory,
                input_digest,
                run_id,
            } => {
                let root = crate::domain::repository_root::RepositoryRoot::discover()?;
                let manifest = VerifyCandidate::new(&self.file_system, &self.repository).execute(
                    &root,
                    &version,
                    &directory,
                    input_digest.as_ref(),
                    run_id.as_deref(),
                )?;
                Ok(format!(
                    "release candidate verified: version {version}, run {}, 20 files, inputs {}",
                    manifest.run_id, manifest.input_sha256
                ))
            }
            ReleaseCommandDto::SyncGuide {
                version,
                root,
                binary,
            } => {
                let engine = self.guide_engines.create(&binary)?;
                if engine.version()? != version {
                    return Err(ReleaseError::invalid(format!(
                        "guide engine is {}, not {version}",
                        engine.version()?
                    )));
                }
                WriteGuideAssets::new(&self.file_system, engine.as_ref()).execute(&root)?;
                Ok(format!(
                    "release guide: human guide, agent guide, plugin and engine agree on {version}"
                ))
            }
            ReleaseCommandDto::WriteGuideAssets { root, binary } => {
                let engine = self.guide_engines.create(&binary)?;
                WriteGuideAssets::new(&self.file_system, engine.as_ref()).execute(&root)?;
                Ok("KMP guide: wrote requests and memory bundle; all probes pass".to_string())
            }
            ReleaseCommandDto::ApplyGuideAssets { root, binary } => {
                let engine = self.guide_engines.create(&binary)?;
                ApplyGuideAssets::new(&self.file_system, engine.as_ref()).execute(&root)?;
                Ok("KMP guide: guide:kmp-agent and guide:kmp converged".to_string())
            }
            ReleaseCommandDto::PackageMcpb {
                version,
                root,
                input,
                output,
            } => {
                let receipt = PackageMcpb::new(&self.file_system, &self.archives)
                    .execute(&root, &version, &input, &output)?;
                Ok(format!(
                    "MCPB ready at {} ({})",
                    receipt.archive.display(),
                    receipt.digest
                ))
            }
            ReleaseCommandDto::StampServerMcpb {
                root,
                archive,
                server_manifest,
            } => {
                let (identifier, digest) = StampServerMcpb::new(&self.file_system).execute(
                    &root,
                    &archive,
                    &server_manifest,
                )?;
                Ok(format!(
                    "stamped {} with {identifier} ({digest})",
                    server_manifest.display()
                ))
            }
            ReleaseCommandDto::PackagePlugin {
                root,
                binary,
                output,
                kind,
            } => {
                let receipt = PackagePlugin::new(
                    &self.file_system,
                    &self.repository,
                    &self.guide_engines,
                    &self.plugin_archives,
                )
                .execute(&root, &binary, &output, kind)?;
                Ok(format!(
                    "KMP plugin package: {}\nKMP plugin package version: {} ({})",
                    receipt.archive.display(),
                    receipt.version,
                    receipt.digest
                ))
            }
            ReleaseCommandDto::VerifyMarketplace {
                version,
                root,
                repository,
                expected_commit,
                allow_unpublished_tag,
            } => {
                let commit = VerifyMarketplace::new(&self.file_system, &self.repository).execute(
                    &root,
                    &version,
                    &repository,
                    expected_commit.as_ref(),
                    allow_unpublished_tag,
                )?;
                Ok(format!(
                    "marketplace parity verified: kmp@underpass Claude={version}, Codex={version}, commit={commit}"
                ))
            }
            ReleaseCommandDto::CurrentVersion { root } => {
                Ok(ReadWorkspaceVersion::new(&self.file_system)
                    .execute(&root)?
                    .to_string())
            }
            ReleaseCommandDto::PrepareVersion { version, root } => {
                let receipt =
                    PrepareReleaseVersion::new(&self.file_system).execute(&root, &version)?;
                Ok(format!(
                    "prepared {version}: {} internal dependency pins; MCPB hash {}",
                    receipt.internal_dependencies(),
                    if receipt.mcpb_hash_was_reset() {
                        "reset"
                    } else {
                        "retained"
                    }
                ))
            }
        }
    }
}
