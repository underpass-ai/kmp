use kmp_release::adapters::current_binary_release_contracts::CurrentBinaryReleaseContracts;
use kmp_release::adapters::gh_candidate_automation::GhCandidateAutomation;
use kmp_release::adapters::git_cli::GitCli;
use kmp_release::adapters::gzip_tar_plugin_archive_writer::GzipTarPluginArchiveWriter;
use kmp_release::adapters::kmp_binary_guide_engine_factory::KmpBinaryGuideEngineFactory;
use kmp_release::adapters::system_environment::SystemEnvironment;
use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::adapters::system_release_workspace::SystemReleaseWorkspace;
use kmp_release::adapters::zip_release_archive_writer::ZipReleaseArchiveWriter;
use kmp_release::application::mappers::release_command_mapper::ReleaseCommandMapper;
use kmp_release::application::mappers::release_workflow_command_mapper::ReleaseWorkflowCommandMapper;
use kmp_release::application::release_application::ReleaseApplication;
use kmp_release::application::release_workflow_application::ReleaseWorkflowApplication;
use kmp_release::domain::repository_root::RepositoryRoot;

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) == Some("workflow") {
        execute_workflow(&arguments[1..]);
        return;
    }
    let command = match ReleaseCommandMapper::map(arguments) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(2);
        }
    };
    let application = ReleaseApplication::new(
        SystemFileSystem,
        GitCli,
        SystemEnvironment,
        KmpBinaryGuideEngineFactory,
        ZipReleaseArchiveWriter,
        GzipTarPluginArchiveWriter,
    );
    match application.execute(command) {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(1);
        }
    }
}

fn execute_workflow(arguments: &[String]) {
    let command = match ReleaseWorkflowCommandMapper::map(arguments) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(2);
        }
    };
    let root = match RepositoryRoot::discover() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(1);
        }
    };
    let contracts = match CurrentBinaryReleaseContracts::new(root.clone()) {
        Ok(contracts) => contracts,
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(1);
        }
    };
    let workspace = SystemReleaseWorkspace::new(root.clone());
    let candidates = GhCandidateAutomation::new(root.clone());
    let application = ReleaseWorkflowApplication::new(
        &SystemFileSystem,
        &contracts,
        &workspace,
        &candidates,
        &root,
    );
    match application.execute(command) {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("kmp-release: {error}");
            std::process::exit(1);
        }
    }
}
