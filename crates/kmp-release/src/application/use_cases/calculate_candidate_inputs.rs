use sha2::{Digest, Sha256};

use crate::domain::candidate_input_digest::CandidateInputDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_repository::ReleaseRepository;

pub struct CalculateCandidateInputs<'a, F, R> {
    file_system: &'a F,
    repository: &'a R,
}

impl<'a, F: CandidateFileSystem, R: ReleaseRepository> CalculateCandidateInputs<'a, F, R> {
    pub fn new(file_system: &'a F, repository: &'a R) -> Self {
        Self {
            file_system,
            repository,
        }
    }

    pub fn execute(&self, root: &RepositoryRoot) -> Result<CandidateInputDigest, ReleaseError> {
        let exact = [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "LICENSE",
            "NOTICE",
            "THIRD_PARTY_NOTICES.md",
            ".github/workflows/release.yml",
            ".agents/plugins/marketplace.json",
            ".claude-plugin/marketplace.json",
            "scripts/ci/install-protoc.sh",
            "scripts/ci/install-rust-toolchain.sh",
        ];
        let prefixes = [
            "crates/",
            "api/",
            ".github/actions/install-rust/",
            "distribution/mcpb/",
            "plugins/kmp/",
            "scripts/plugin/",
        ];
        let mut selected = self
            .repository
            .tracked_files(root)?
            .into_iter()
            .filter(|path| {
                let relative = path.to_string_lossy();
                exact.contains(&relative.as_ref())
                    || prefixes.iter().any(|prefix| relative.starts_with(prefix))
            })
            .collect::<Vec<_>>();
        selected.sort();
        let mut digest = Sha256::new();
        for relative in selected {
            let path_bytes = relative.to_string_lossy().as_bytes().to_vec();
            let content = self.file_system.read_bytes(&root.join(&relative))?;
            digest.update(
                u64::try_from(path_bytes.len())
                    .map_err(|_| ReleaseError::invalid("release input path is too large"))?
                    .to_be_bytes(),
            );
            digest.update(&path_bytes);
            digest.update(
                u64::try_from(content.len())
                    .map_err(|_| ReleaseError::invalid("release input is too large"))?
                    .to_be_bytes(),
            );
            digest.update(&content);
        }
        Ok(CandidateInputDigest::from_bytes(digest.finalize().into()))
    }
}
