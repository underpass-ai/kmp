use crate::lifecycle::application::dto::lifecycle_bridge_dto::LifecycleBridgeDto;
use crate::lifecycle::application::dto::lifecycle_cache_dto::LifecycleCacheDto;
use crate::lifecycle::application::dto::lifecycle_engine_dto::LifecycleEngineDto;
use crate::lifecycle::application::dto::lifecycle_host_dto::LifecycleHostDto;
use crate::lifecycle::application::dto::lifecycle_receipt_dto::LifecycleReceiptDto;
use crate::lifecycle::domain::bridge_installation::BridgeInstallation;
use crate::lifecycle::domain::convergence_status::ConvergenceStatus;
use crate::lifecycle::domain::lifecycle_action::LifecycleAction;
use crate::lifecycle::domain::lifecycle_receipt::LifecycleReceipt;

/// Maps a proved domain outcome onto the stable CLI DTO.
#[derive(Clone, Copy, Debug, Default)]
pub struct LifecycleReceiptMapper;

impl LifecycleReceiptMapper {
    pub fn to_dto(receipt: &LifecycleReceipt) -> LifecycleReceiptDto {
        let action = match receipt.action() {
            LifecycleAction::Setup => "setup",
            LifecycleAction::Update => "update",
        };
        LifecycleReceiptDto {
            action: action.to_string(),
            status: if receipt.is_dry_run() {
                "planned"
            } else {
                "completed"
            }
            .to_string(),
            version: receipt.version().to_string(),
            dry_run: receipt.is_dry_run(),
            hosts: receipt
                .hosts()
                .iter()
                .map(|host| LifecycleHostDto {
                    host: host.host().to_string(),
                    status: match host.status() {
                        ConvergenceStatus::PlannedChange => "planned_change",
                        ConvergenceStatus::Changed => "changed",
                        ConvergenceStatus::Unchanged => "unchanged",
                        ConvergenceStatus::Skipped => "skipped",
                    }
                    .to_string(),
                    previous_version: host.previous_version().map(ToString::to_string),
                    version: host.version().to_string(),
                    root: host.root().map(|root| root.as_path().display().to_string()),
                    enabled: host.is_enabled(),
                    warning: host.warning().map(str::to_string),
                })
                .collect(),
            engines: receipt
                .engine_proofs()
                .iter()
                .map(|host_proof| LifecycleEngineDto {
                    consumer: host_proof.host().to_string(),
                    executable: host_proof
                        .proof()
                        .executable()
                        .as_path()
                        .display()
                        .to_string(),
                    version: host_proof.proof().version().to_string(),
                    tool_count: host_proof.proof().tool_count(),
                })
                .collect(),
            plugin_tree_digest: receipt.plugin_tree().map(ToString::to_string),
            plugin_caches: receipt
                .deferred_caches()
                .iter()
                .map(|(host, deferral)| LifecycleCacheDto {
                    host: host.to_string(),
                    deferred: deferral
                        .deferred()
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                })
                .collect(),
            lexical_bridge: receipt.lexical_bridge().map(Self::bridge_dto),
        }
    }

    fn bridge_dto(installation: &BridgeInstallation) -> LifecycleBridgeDto {
        let (outcome, path, sha256, replaced_sha256) = match installation {
            BridgeInstallation::Installed {
                path,
                sha256,
                replaced,
                ..
            } => (
                "installed",
                Some(path.display().to_string()),
                Some(sha256.clone()),
                replaced.clone(),
            ),
            BridgeInstallation::AlreadyCurrent { path, sha256 } => (
                "already_current",
                Some(path.display().to_string()),
                Some(sha256.clone()),
                None,
            ),
            BridgeInstallation::Declined => ("declined", None, None, None),
            BridgeInstallation::Unavailable { .. } => ("unavailable", None, None, None),
        };
        LifecycleBridgeDto {
            outcome: outcome.to_string(),
            detail: installation.summary(),
            path,
            sha256,
            replaced_sha256,
            crosses_languages: installation.table_is_present(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::domain::host::Host;
    use crate::lifecycle::domain::host_convergence::HostConvergence;
    use crate::lifecycle::domain::release_version::ReleaseVersion;

    #[test]
    fn a_skipped_host_reaches_the_receipt_with_its_warning() {
        let receipt = LifecycleReceipt::planned(
            LifecycleAction::Setup,
            ReleaseVersion::current(),
            vec![HostConvergence::skipped_without_package(
                Host::Pi,
                "underpass-pi",
                None,
                ReleaseVersion::current(),
            )],
        );

        let dto = LifecycleReceiptMapper::to_dto(&receipt);
        let pi = &dto.hosts[0];
        assert_eq!(pi.host, "pi");
        assert_eq!(pi.status, "skipped");
        assert!(!pi.enabled);
        assert_eq!(
            pi.warning.as_deref(),
            Some("pi present but underpass-pi not registered; run `underpass setup`")
        );
        let json = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            json["hosts"][0]["warning"],
            "pi present but underpass-pi not registered; run `underpass setup`"
        );
    }

    #[test]
    fn a_host_without_a_warning_serializes_none() {
        let receipt = LifecycleReceipt::planned(
            LifecycleAction::Update,
            ReleaseVersion::current(),
            vec![HostConvergence::planned(
                LifecycleAction::Update,
                Host::Codex,
                None,
                ReleaseVersion::current(),
            )],
        );

        let json = serde_json::to_value(LifecycleReceiptMapper::to_dto(&receipt)).expect("json");
        assert!(json["hosts"][0].get("warning").is_none());
    }
}
