//! Native installation lifecycle bounded context.
//!
//! Domain decisions do not know about subprocesses, HTTP or filesystem
//! layout. Application orchestration talks only to ports; adapters own the
//! host CLIs, GitHub and durable executable replacement.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;

pub use adapters::filesystem_engine_store::FilesystemEngineStore;
pub use adapters::github_release_repository::GithubReleaseRepository;
pub use adapters::lifecycle_cli_parser::LifecycleCliParser;
pub use adapters::native_host_gateway::NativeHostGateway;
pub use adapters::native_lifecycle::NativeLifecycle;
pub use adapters::native_plugin_engine_resolver::NativePluginEngineResolver;
pub use adapters::system_process_executor::SystemProcessExecutor;
pub use application::dto::lifecycle_command_dto::LifecycleCommandDto;
pub use application::dto::lifecycle_failure_dto::LifecycleFailureDto;
pub use application::dto::lifecycle_receipt_dto::LifecycleReceiptDto;
pub use application::dto::plugin_engine_resolution_dto::PluginEngineResolutionDto;
pub use application::mappers::lifecycle_command_mapper::LifecycleCommandMapper;
pub use application::mappers::lifecycle_failure_mapper::LifecycleFailureMapper;
pub use application::mappers::lifecycle_receipt_mapper::LifecycleReceiptMapper;
pub use application::use_cases::diagnose_lifecycle::DiagnoseLifecycle;
pub use application::use_cases::setup_kmp::SetupKmp;
pub use application::use_cases::update_kmp::UpdateKmp;
pub use domain::engine_install_dir::EngineInstallDir;
pub use domain::host::Host;
pub use domain::lifecycle_action::LifecycleAction;
pub use domain::lifecycle_error::LifecycleError;
pub use domain::lifecycle_receipt::LifecycleReceipt;
pub use domain::lifecycle_request::LifecycleRequest;
pub use domain::plugin_root::PluginRoot;
pub use domain::release_version::ReleaseVersion;
