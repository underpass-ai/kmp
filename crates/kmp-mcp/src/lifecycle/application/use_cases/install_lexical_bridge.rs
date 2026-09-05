use crate::lifecycle::domain::bridge_choice::BridgeChoice;
use crate::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use crate::lifecycle::domain::bridge_installation::BridgeInstallation;
use crate::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::bridge_store::BridgeStore;
use crate::lifecycle::ports::release_repository::ReleaseRepository;

/// Use case: give this machine the table that lets `ask` cross languages.
///
/// It cannot fail. A convergence that installed the engine, proved its tools
/// and pointed every host at it has succeeded whether or not a retrieval aid
/// came with it, so every way this can go wrong becomes a reported outcome
/// rather than an error. The one thing it will not do is fail quietly: the
/// receipt says which of them happened.
pub struct InstallLexicalBridge<'a> {
    releases: &'a dyn ReleaseRepository,
    tables: &'a dyn BridgeStore,
}

impl<'a> InstallLexicalBridge<'a> {
    pub fn new(releases: &'a dyn ReleaseRepository, tables: &'a dyn BridgeStore) -> Self {
        Self { releases, tables }
    }

    pub fn execute(
        &self,
        choice: &BridgeChoice,
        destination: &BridgeInstallDir,
        version: &ReleaseVersion,
    ) -> BridgeInstallation {
        match choice {
            BridgeChoice::Declined => BridgeInstallation::Declined,
            BridgeChoice::FromFile(path) => self
                .install(self.tables.read(path), destination)
                .unwrap_or_else(BridgeInstallation::unavailable),
            BridgeChoice::FromRelease => self
                .install_published(destination, version)
                .unwrap_or_else(BridgeInstallation::unavailable),
        }
    }

    /// The published digest decides before any megabyte moves: a machine that
    /// already holds this table downloads the checksum and nothing else.
    fn install_published(
        &self,
        destination: &BridgeInstallDir,
        version: &ReleaseVersion,
    ) -> Result<BridgeInstallation, LifecycleError> {
        let Some(published) = self.releases.lexical_bridge_checksum(version)? else {
            return Ok(BridgeInstallation::Unavailable {
                reason: format!("release {} publishes no table", version.tag()),
            });
        };
        if self.tables.installed_digest(destination).as_deref() == Some(published.as_str()) {
            return Ok(BridgeInstallation::AlreadyCurrent {
                path: destination.table(),
                sha256: published,
            });
        }
        self.install(self.releases.lexical_bridge(version), destination)
    }

    fn install(
        &self,
        artifact: Result<LexicalBridgeArtifact, LifecycleError>,
        destination: &BridgeInstallDir,
    ) -> Result<BridgeInstallation, LifecycleError> {
        let artifact = artifact?;
        if self.tables.installed_digest(destination).as_deref() == Some(artifact.sha256()) {
            return Ok(BridgeInstallation::AlreadyCurrent {
                path: destination.table(),
                sha256: artifact.sha256().to_string(),
            });
        }
        let path = self.tables.install(&artifact, destination)?;
        Ok(BridgeInstallation::Installed {
            path,
            bytes: artifact.len(),
            sha256: artifact.sha256().to_string(),
            source: artifact.source().to_string(),
        })
    }
}
