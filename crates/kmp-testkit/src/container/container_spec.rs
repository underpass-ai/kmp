use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::ContainerError;
use super::container_cli::ContainerCli;
use super::running_container::RunningContainer;
use super::wait_for::WaitFor;

/// How long a container may take to satisfy its wait conditions.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const LOG_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Marks every container the harness creates, so strays are easy to find.
const OWNER_LABEL: &str = "org.underpass.kmp.testkit=true";

/// Description of a test container: image, environment, command, the ports a
/// test reaches from the host, the files it needs before it boots and what it
/// must print before it counts as ready.
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    image: String,
    entrypoint: Option<String>,
    cmd: Vec<String>,
    env: Vec<(String, String)>,
    exposed_ports: Vec<u16>,
    copies: Vec<(String, PathBuf)>,
    waits: Vec<WaitFor>,
    log_prefix: Option<String>,
}

impl ContainerSpec {
    pub fn new(image: impl AsRef<str>, tag: impl AsRef<str>) -> Self {
        Self {
            image: format!("{}:{}", image.as_ref(), tag.as_ref()),
            entrypoint: None,
            cmd: Vec::new(),
            env: Vec::new(),
            exposed_ports: Vec::new(),
            copies: Vec::new(),
            waits: Vec::new(),
            log_prefix: None,
        }
    }

    /// Publishes a container TCP port on an ephemeral host port.
    pub fn with_exposed_port(mut self, port: u16) -> Self {
        self.exposed_ports.push(port);
        self
    }

    pub fn with_env_var(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = Some(entrypoint.into());
        self
    }

    pub fn with_cmd<I, S>(mut self, cmd: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.cmd = cmd.into_iter().map(Into::into).collect();
        self
    }

    /// Copies a host file or directory to `target` before the container
    /// starts; a directory's contents become the contents of `target`.
    pub fn with_copy_to(mut self, target: impl Into<String>, source: impl AsRef<Path>) -> Self {
        self.copies
            .push((target.into(), source.as_ref().to_path_buf()));
        self
    }

    pub fn with_wait_for(mut self, wait: WaitFor) -> Self {
        self.waits.push(wait);
        self
    }

    /// Mirrors the container's output to stderr, each line tagged `[prefix]`.
    pub fn with_log_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.log_prefix = Some(prefix.into());
        self
    }

    /// Creates, provisions and starts the container, then waits until it is
    /// ready. On any failure the container is removed before returning.
    pub async fn start(self) -> Result<RunningContainer, ContainerError> {
        let cli = ContainerCli::selected()?;
        let id = cli.run(self.create_args())?;
        let container = RunningContainer::new(cli.clone(), id);

        for (target, source) in &self.copies {
            let mut source = source.as_os_str().to_os_string();
            if Path::new(&source).is_dir() {
                source.push("/.");
            }
            let destination = format!("{}:{target}", container.id());
            cli.run([OsStr::new("cp"), source.as_os_str(), destination.as_ref()])
                .map_err(|error| prefixed("copy", error))?;
        }
        cli.run(["start", container.id()])?;
        if let Some(prefix) = &self.log_prefix {
            container.mirror_logs(prefix.clone());
        }

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        for wait in &self.waits {
            match wait {
                WaitFor::Duration(duration) => tokio::time::sleep(*duration).await,
                WaitFor::StdoutMessage(message) => {
                    wait_for_log(&cli, &container, message, true, deadline).await?;
                }
                WaitFor::StderrMessage(message) => {
                    wait_for_log(&cli, &container, message, false, deadline).await?;
                }
            }
        }
        Ok(container)
    }

    fn create_args(&self) -> Vec<String> {
        let mut args = vec![
            "create".to_string(),
            "--label".to_string(),
            OWNER_LABEL.to_string(),
        ];
        if let Some(entrypoint) = &self.entrypoint {
            args.extend(["--entrypoint".to_string(), entrypoint.clone()]);
        }
        for (name, value) in &self.env {
            args.extend(["--env".to_string(), format!("{name}={value}")]);
        }
        for port in &self.exposed_ports {
            args.extend(["--publish".to_string(), format!("{port}/tcp")]);
        }
        args.push(self.image.clone());
        args.extend(self.cmd.iter().cloned());
        args
    }
}

fn prefixed(step: &str, error: ContainerError) -> ContainerError {
    std::io::Error::other(format!("container {step} failed: {error}")).into()
}

async fn wait_for_log(
    cli: &ContainerCli,
    container: &RunningContainer,
    message: &str,
    stdout: bool,
    deadline: Instant,
) -> Result<(), ContainerError> {
    let stream = if stdout { "stdout" } else { "stderr" };
    loop {
        let logs = cli.output(["logs", container.id()])?;
        let text = if stdout { &logs.stdout } else { &logs.stderr };
        if String::from_utf8_lossy(text).contains(message) {
            return Ok(());
        }
        if !container.is_running()? {
            return Err(std::io::Error::other(format!(
                "container {} ({}) exited before printing `{message}` on {stream}; output:\n{}\n{}",
                container.id(),
                cli.program(),
                String::from_utf8_lossy(&logs.stdout),
                String::from_utf8_lossy(&logs.stderr)
            ))
            .into());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "container {} did not print `{message}` on {stream} within {STARTUP_TIMEOUT:?}",
                    container.id()
                ),
            )
            .into());
        }
        tokio::time::sleep(LOG_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_args_carry_entrypoint_env_ports_image_and_cmd_in_order() {
        let spec = ContainerSpec::new("docker.io/library/nats", "2.10-alpine")
            .with_entrypoint("nats-server")
            .with_env_var("A", "1")
            .with_exposed_port(4222)
            .with_cmd(["-js", "-c", "/tls/nats.conf"]);
        assert_eq!(
            spec.create_args(),
            [
                "create",
                "--label",
                OWNER_LABEL,
                "--entrypoint",
                "nats-server",
                "--env",
                "A=1",
                "--publish",
                "4222/tcp",
                "docker.io/library/nats:2.10-alpine",
                "-js",
                "-c",
                "/tls/nats.conf",
            ]
        );
    }

    #[test]
    fn a_bare_spec_creates_the_image_with_its_own_command() {
        let spec = ContainerSpec::new("docker.io/valkey/valkey", "8.1.5-alpine");
        assert_eq!(
            spec.create_args(),
            [
                "create",
                "--label",
                OWNER_LABEL,
                "docker.io/valkey/valkey:8.1.5-alpine"
            ]
        );
    }

    #[test]
    fn wait_conditions_and_copies_are_kept_in_order() {
        let spec = ContainerSpec::new("img", "tag")
            .with_wait_for(WaitFor::seconds(2))
            .with_wait_for(WaitFor::message_on_stdout("Ready"))
            .with_copy_to("/tls", "/tmp/material");
        assert_eq!(
            spec.waits,
            [
                WaitFor::Duration(Duration::from_secs(2)),
                WaitFor::StdoutMessage("Ready".to_string())
            ]
        );
        assert_eq!(
            spec.copies,
            [("/tls".to_string(), PathBuf::from("/tmp/material"))]
        );
    }
}
