use crate::lifecycle::adapters::filesystem_engine_store::FilesystemEngineStore;
use crate::lifecycle::adapters::github_release_repository::GithubReleaseRepository;
use crate::lifecycle::adapters::lifecycle_cli_parser::LifecycleCliParser;
use crate::lifecycle::adapters::native_host_gateway::NativeHostGateway;
use crate::lifecycle::adapters::system_process_executor::SystemProcessExecutor;
use crate::lifecycle::application::dto::lifecycle_receipt_dto::LifecycleReceiptDto;
use crate::lifecycle::application::mappers::lifecycle_command_mapper::LifecycleCommandMapper;
use crate::lifecycle::application::mappers::lifecycle_receipt_mapper::LifecycleReceiptMapper;
use crate::lifecycle::application::use_cases::diagnose_lifecycle::DiagnoseLifecycle;
use crate::lifecycle::application::use_cases::setup_kmp::SetupKmp;
use crate::lifecycle::application::use_cases::update_kmp::UpdateKmp;
use crate::lifecycle::domain::lifecycle_action::LifecycleAction;
use crate::lifecycle::domain::lifecycle_diagnosis::LifecycleDiagnosis;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Native composition adapter. Entrypoints depend on this facade; domain and
/// application code remain unaware of concrete processes, HTTP and files.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeLifecycle;

impl NativeLifecycle {
    pub fn execute(
        action: LifecycleAction,
        arguments: &[&str],
    ) -> Result<LifecycleReceiptDto, LifecycleError> {
        let command = LifecycleCliParser::parse(arguments)?;
        let request = LifecycleCommandMapper::to_domain(command, action)?;
        let processes = SystemProcessExecutor::default();
        let hosts = NativeHostGateway::new(&processes);
        let releases = GithubReleaseRepository::new()?;
        let engines = FilesystemEngineStore;
        let receipt = match action {
            LifecycleAction::Setup => {
                SetupKmp::new(&hosts, &releases, &engines).execute(request)?
            }
            LifecycleAction::Update => {
                UpdateKmp::new(&hosts, &releases, &engines).execute(request)?
            }
        };
        Ok(LifecycleReceiptMapper::to_dto(&receipt))
    }

    pub fn diagnose() -> LifecycleDiagnosis {
        let processes = SystemProcessExecutor::default();
        let hosts = NativeHostGateway::new(&processes);
        let engines = FilesystemEngineStore;
        DiagnoseLifecycle::new(&hosts, &engines).execute()
    }
}
