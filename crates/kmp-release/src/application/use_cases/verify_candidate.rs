use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::application::dto::candidate_manifest_dto::CandidateManifestDto;
use crate::application::use_cases::calculate_candidate_inputs::CalculateCandidateInputs;
use crate::domain::candidate_asset_set::CandidateAssetSet;
use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_repository::ReleaseRepository;

pub struct VerifyCandidate<'a, F, R> {
    file_system: &'a F,
    repository: &'a R,
}

impl<'a, F: CandidateFileSystem, R: ReleaseRepository> VerifyCandidate<'a, F, R> {
    pub fn new(file_system: &'a F, repository: &'a R) -> Self {
        Self {
            file_system,
            repository,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
        version: &ReleaseVersion,
        directory: &Path,
        expected_input: Option<&CandidateInputDigest>,
        expected_run_id: Option<&str>,
    ) -> Result<CandidateManifestDto, ReleaseError> {
        let manifest: CandidateManifestDto = serde_json::from_slice(
            &self
                .file_system
                .read_bytes(&directory.join("candidate.json"))?,
        )
        .map_err(|error| ReleaseError::invalid(format!("candidate.json is invalid: {error}")))?;
        if manifest.contract != "kmp.release-candidate.v1" {
            return Err(ReleaseError::invalid(format!(
                "unexpected candidate contract: {}",
                manifest.contract
            )));
        }
        if manifest.version != version.as_str() {
            return Err(ReleaseError::invalid(format!(
                "candidate version {} does not match {version}",
                manifest.version
            )));
        }
        let calculated_input;
        let expected_input = match expected_input {
            Some(value) => value,
            None => {
                calculated_input = CalculateCandidateInputs::new(self.file_system, self.repository)
                    .execute(root)?;
                &calculated_input
            }
        };
        if manifest.input_sha256 != expected_input.as_str() {
            return Err(ReleaseError::invalid(format!(
                "candidate release inputs differ: candidate={} current={expected_input}",
                manifest.input_sha256
            )));
        }
        if expected_run_id.is_some_and(|run_id| manifest.run_id != run_id) {
            return Err(ReleaseError::invalid(format!(
                "candidate run {} does not match approved run {}",
                manifest.run_id,
                expected_run_id.unwrap_or_default()
            )));
        }
        let asset_set = CandidateAssetSet::for_version(version);
        let assets_dir = directory.join("assets");
        let mut actual = self
            .file_system
            .walk_files(&assets_dir)?
            .into_iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .collect::<Vec<_>>();
        actual.sort();
        if !asset_set.matches(&actual) {
            return Err(ReleaseError::invalid(
                "candidate asset set differs from the release contract",
            ));
        }
        let records = manifest
            .assets
            .iter()
            .map(|record| (record.name.as_str(), record))
            .collect::<BTreeMap<_, _>>();
        for name in asset_set.payloads() {
            let asset = assets_dir.join(name);
            let digest = CandidateInputDigest::from_bytes(
                Sha256::digest(self.file_system.read_bytes(&asset)?).into(),
            );
            let checksum_text = String::from_utf8(
                self.file_system
                    .read_bytes(&assets_dir.join(format!("{name}.sha256")))?,
            )
            .map_err(|error| {
                ReleaseError::invalid(format!("checksum for {name} is not UTF-8: {error}"))
            })?;
            if checksum_text.split_whitespace().next() != Some(digest.as_str()) {
                return Err(ReleaseError::invalid(format!(
                    "candidate checksum does not match {name}"
                )));
            }
            let record = records.get(name).ok_or_else(|| {
                ReleaseError::invalid(format!("candidate manifest does not describe {name}"))
            })?;
            if record.sha256 != digest.as_str()
                || record.size != self.file_system.file_size(&asset)?
            {
                return Err(ReleaseError::invalid(format!(
                    "candidate manifest does not describe {name}"
                )));
            }
        }
        let server: serde_json::Value =
            serde_json::from_slice(&self.file_system.read_bytes(&root.join("server.json"))?)
                .map_err(|error| {
                    ReleaseError::invalid(format!("server.json is invalid: {error}"))
                })?;
        let declared = server["packages"]
            .as_array()
            .and_then(|packages| {
                packages
                    .iter()
                    .find(|package| package["registryType"] == "mcpb")
            })
            .and_then(|package| package["fileSha256"].as_str())
            .ok_or_else(|| ReleaseError::invalid("server.json has no MCPB fileSha256"))?;
        let mcpb_name = format!("kmp-mcp-v{version}.mcpb");
        let mcpb_digest = CandidateInputDigest::from_bytes(
            Sha256::digest(self.file_system.read_bytes(&assets_dir.join(mcpb_name))?).into(),
        );
        if declared != mcpb_digest.as_str() {
            return Err(ReleaseError::invalid(format!(
                "server.json MCPB hash {declared} does not match candidate {mcpb_digest}"
            )));
        }
        Ok(manifest)
    }
}
