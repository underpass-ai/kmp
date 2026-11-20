//! Throwaway containers for integration tests, driven through the container
//! CLI (`docker`, or `podman` where Docker is absent).
//!
//! A test describes the container with [`ContainerSpec`], starts it, and holds
//! the [`RunningContainer`] for as long as it needs the service; dropping it
//! removes the container. Talking to the CLI keeps the test harness free of a
//! container-engine client library and of the archive crates it pulls in.

mod container_cli;
mod container_spec;
mod running_container;
mod wait_for;

pub use container_cli::ensure_container_runtime;
pub use container_spec::ContainerSpec;
pub use running_container::RunningContainer;
pub use wait_for::WaitFor;

/// The error every container operation reports.
pub type ContainerError = Box<dyn std::error::Error + Send + Sync>;
