use crate::lifecycle::application::dto::lifecycle_bridge_dto::LifecycleBridgeDto;
use crate::lifecycle::application::dto::lifecycle_cache_dto::LifecycleCacheDto;
use crate::lifecycle::application::dto::lifecycle_engine_dto::LifecycleEngineDto;
use crate::lifecycle::application::dto::lifecycle_host_dto::LifecycleHostDto;
use crate::lifecycle::application::dto::lifecycle_receipt_dto::LifecycleReceiptDto;
use crate::lifecycle::domain::bridge_installation::BridgeInstallation;
use crate::lifecycle::domain::convergence_status::ConvergenceStatus;
use crate::lifecycle::domain::host::Host;
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
                    host: match host.host() {
                        Host::Claude => "claude",
                        Host::Codex => "codex",
                    }
                    .to_string(),
                    status: match host.status() {
                        ConvergenceStatus::PlannedChange => "planned_change",
                        ConvergenceStatus::Changed => "changed",
                        ConvergenceStatus::Unchanged => "unchanged",
                    }
                    .to_string(),
                    previous_version: host.previous_version().map(ToString::to_string),
                    version: host.version().to_string(),
                    root: host.root().map(|root| root.as_path().display().to_string()),
                    enabled: host.is_enabled(),
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
                .pruned_caches()
                .iter()
                .map(|(host, pruning)| LifecycleCacheDto {
                    host: host.to_string(),
                    removed: pruning.removed().iter().map(ToString::to_string).collect(),
                    kept: pruning.kept().iter().map(ToString::to_string).collect(),
                })
                .collect(),
            lexical_bridge: receipt.lexical_bridge().map(Self::bridge_dto),
        }
    }

    fn bridge_dto(installation: &BridgeInstallation) -> LifecycleBridgeDto {
        let (outcome, path, sha256) = match installation {
            BridgeInstallation::Installed { path, sha256, .. } => (
                "installed",
                Some(path.display().to_string()),
                Some(sha256.clone()),
            ),
            BridgeInstallation::AlreadyCurrent { path, sha256 } => (
                "already_current",
                Some(path.display().to_string()),
                Some(sha256.clone()),
            ),
            BridgeInstallation::Declined => ("declined", None, None),
            BridgeInstallation::Unavailable { .. } => ("unavailable", None, None),
        };
        LifecycleBridgeDto {
            outcome: outcome.to_string(),
            detail: installation.summary(),
            path,
            sha256,
            crosses_languages: installation.table_is_present(),
        }
    }
}
