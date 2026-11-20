use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use super::ContainerError;
use super::container_cli::ContainerCli;

/// A started test container. Dropping it removes the container and its
/// anonymous volumes.
#[derive(Debug)]
pub struct RunningContainer {
    cli: ContainerCli,
    id: String,
    host: String,
}

impl RunningContainer {
    pub(super) fn new(cli: ContainerCli, id: String) -> Self {
        Self {
            cli,
            id,
            host: engine_host(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The host name tests use to reach published ports: the engine's host
    /// for a TCP `DOCKER_HOST`, `localhost` otherwise.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The IPv4 host port the engine published for a container TCP port.
    pub fn host_port_ipv4(&self, port: u16) -> Result<u16, ContainerError> {
        let mappings = self.cli.run(["port", &self.id, &format!("{port}/tcp")])?;
        ipv4_host_port(&mappings).ok_or_else(|| {
            std::io::Error::other(format!(
                "container {} publishes no IPv4 host port for {port}/tcp: `{mappings}`",
                self.id
            ))
            .into()
        })
    }

    pub(super) fn is_running(&self) -> Result<bool, ContainerError> {
        Ok(self
            .cli
            .run(["inspect", "--format", "{{.State.Running}}", &self.id])?
            == "true")
    }

    /// Streams the container's output to stderr until the container is gone.
    pub(super) fn mirror_logs(&self, prefix: String) {
        let child = Command::new(self.cli.program())
            .args(["logs", "--follow", &self.id])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let Ok(mut child) = child else {
            return;
        };
        for stream in [
            child
                .stdout
                .take()
                .map(|out| Box::new(out) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|err| Box::new(err) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let prefix = prefix.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    eprintln!("[{prefix}] {}", line.trim_end());
                }
            });
        }
        std::thread::spawn(move || child.wait());
    }
}

impl Drop for RunningContainer {
    fn drop(&mut self) {
        self.cli
            .run_quietly(["rm", "--force", "--volumes", self.id.as_str()]);
    }
}

fn engine_host() -> String {
    host_from_docker_host(std::env::var("DOCKER_HOST").ok().as_deref())
}

/// The host of a TCP `DOCKER_HOST`; `localhost` for a local socket or none.
fn host_from_docker_host(docker_host: Option<&str>) -> String {
    docker_host
        .and_then(|value| {
            let rest = value.strip_prefix("tcp://")?;
            let authority = rest.split('/').next()?;
            let host = authority
                .rsplit_once(':')
                .map_or(authority, |(host, _)| host);
            (!host.is_empty()).then(|| host.to_string())
        })
        .unwrap_or_else(|| "localhost".to_string())
}

/// The first IPv4 mapping in `docker port` output (`0.0.0.0:32768`), skipping
/// IPv6 lines such as `[::]:32768`.
fn ipv4_host_port(mappings: &str) -> Option<u16> {
    mappings
        .lines()
        .map(str::trim)
        .filter(|mapping| !mapping.starts_with('['))
        .find_map(|mapping| mapping.rsplit_once(':')?.1.parse::<u16>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ipv4_mapping_wins_over_ipv6() {
        assert_eq!(ipv4_host_port("[::]:40001\n0.0.0.0:40000\n"), Some(40000));
        assert_eq!(ipv4_host_port("0.0.0.0:32768"), Some(32768));
    }

    #[test]
    fn no_ipv4_mapping_is_none() {
        assert_eq!(ipv4_host_port("[::]:40001"), None);
        assert_eq!(ipv4_host_port(""), None);
    }

    #[test]
    fn a_tcp_docker_host_names_the_engine_host() {
        assert_eq!(
            host_from_docker_host(Some("tcp://10.0.0.5:2375")),
            "10.0.0.5"
        );
        assert_eq!(
            host_from_docker_host(Some("tcp://docker.internal")),
            "docker.internal"
        );
    }

    #[test]
    fn a_local_socket_means_localhost() {
        assert_eq!(
            host_from_docker_host(Some("unix:///run/docker.sock")),
            "localhost"
        );
        assert_eq!(host_from_docker_host(None), "localhost");
    }
}
