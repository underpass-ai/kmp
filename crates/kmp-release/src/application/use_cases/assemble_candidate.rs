use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::application::dto::candidate_asset_dto::CandidateAssetDto;
use crate::application::dto::candidate_manifest_dto::CandidateManifestDto;
use crate::application::use_cases::calculate_candidate_inputs::CalculateCandidateInputs;
use crate::domain::candidate_asset_set::CandidateAssetSet;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_environment::ReleaseEnvironment;
use crate::ports::release_repository::ReleaseRepository;

pub struct AssembleCandidate<'a, F, R, E> {
    file_system: &'a F,
    repository: &'a R,
    environment: &'a E,
}

impl<'a, F: CandidateFileSystem, R: ReleaseRepository, E: ReleaseEnvironment>
    AssembleCandidate<'a, F, R, E>
{
    pub fn new(file_system: &'a F, repository: &'a R, environment: &'a E) -> Self {
        Self {
            file_system,
            repository,
            environment,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
        source_roots: &[PathBuf],
        output: &Path,
    ) -> Result<CandidateManifestDto, ReleaseError> {
        self.file_system.remove_dir_all_if_present(output)?;
        let assets = output.join("assets");
        self.file_system.create_dir_all(&assets)?;
        let asset_set = CandidateAssetSet::for_version(version);
        for name in asset_set.all() {
            let source = self.locate(name, source_roots)?;
            self.file_system.copy_file(&source, &assets.join(name))?;
        }
        let mut records = Vec::new();
        for name in asset_set.payloads() {
            let asset = assets.join(name);
            let checksum = assets.join(format!("{name}.sha256"));
            let digest = self.validate_checksum(&asset, &checksum)?;
            records.push(CandidateAssetDto {
                name: name.to_string(),
                sha256: digest.to_string(),
                size: self.file_system.file_size(&asset)?,
            });
        }
        let input_digest =
            CalculateCandidateInputs::new(self.file_system, self.repository).execute(root)?;
        let source_commit = match self.environment.value("GITHUB_SHA") {
            Some(value) => crate::domain::source_commit::SourceCommit::parse(value)?,
            None => self.repository.head_commit(root)?,
        };
        let manifest = CandidateManifestDto {
            contract: "kmp.release-candidate.v1".to_string(),
            version: version.to_string(),
            input_sha256: input_digest.to_string(),
            source_sha: source_commit.to_string(),
            source_ref: self
                .environment
                .value("GITHUB_REF_NAME")
                .unwrap_or_else(|| "local".to_string()),
            run_id: self
                .environment
                .value("GITHUB_RUN_ID")
                .unwrap_or_else(|| "local".to_string()),
            assets: records,
        };
        let body = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            ReleaseError::invalid(format!("could not serialize candidate manifest: {error}"))
        })?;
        let mut body = body;
        body.push(b'\n');
        self.file_system
            .write_bytes(&output.join("candidate.json"), &body)?;
        Ok(manifest)
    }

    fn locate(&self, name: &str, roots: &[PathBuf]) -> Result<PathBuf, ReleaseError> {
        let mut matches = Vec::new();
        for root in roots {
            matches.extend(
                self.file_system
                    .walk_files(root)?
                    .into_iter()
                    .filter(|path| path.file_name().is_some_and(|file| file == name)),
            );
        }
        matches.sort();
        matches.dedup();
        if matches.len() != 1 {
            return Err(ReleaseError::invalid(format!(
                "candidate expected exactly one {name}, found {}",
                matches.len()
            )));
        }
        Ok(matches.remove(0))
    }

    fn validate_checksum(
        &self,
        asset: &Path,
        checksum: &Path,
    ) -> Result<CandidateInputDigest, ReleaseError> {
        let expected = String::from_utf8(self.file_system.read_bytes(checksum)?)
            .map_err(|error| {
                ReleaseError::invalid(format!("{} is not UTF-8: {error}", checksum.display()))
            })?
            .split_whitespace()
            .next()
            .ok_or_else(|| ReleaseError::invalid(format!("{} is empty", checksum.display())))?
            .to_string();
        let actual = CandidateInputDigest::from_bytes(
            Sha256::digest(self.file_system.read_bytes(asset)?).into(),
        );
        if expected != actual.as_str() {
            return Err(ReleaseError::invalid(format!(
                "candidate checksum does not match {}",
                asset.display()
            )));
        }
        Ok(actual)
    }
}
