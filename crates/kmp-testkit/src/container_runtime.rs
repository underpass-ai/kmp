use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

static TESTCONTAINERS_RUNTIME_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn ensure_testcontainers_runtime() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    TESTCONTAINERS_RUNTIME_INIT
        .get_or_init(|| configure_testcontainers_runtime().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| {
            Box::new(std::io::Error::other(message.clone()))
                as Box<dyn std::error::Error + Send + Sync>
        })?;

    Ok(())
}

fn configure_testcontainers_runtime() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if docker_is_available() || !command_exists("podman") {
        return Ok(());
    }

    let managed_socket = managed_docker_socket_path()?;

    if path_is_socket(&managed_socket)? {
        return Ok(());
    }

    if let Some(podman_socket) = existing_podman_socket() {
        link_managed_socket(&managed_socket, &podman_socket)?;
        return Ok(());
    }

    if command_exists("systemctl") {
        let _ = Command::new("systemctl")
            .args(["--user", "start", "podman.socket"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if let Some(podman_socket) = wait_for_podman_socket() {
            link_managed_socket(&managed_socket, &podman_socket)?;
            return Ok(());
        }
    }

    start_podman_service(&managed_socket)
}

fn docker_is_available() -> bool {
    if !command_exists("docker") {
        return false;
    }

    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args(["-lc", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}

fn managed_docker_socket_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?;
    let socket_path = home.join(".docker/run/docker.sock");
    let parent = socket_path
        .parent()
        .expect("managed socket path should have a parent");
    fs::create_dir_all(parent)?;
    Ok(socket_path)
}

fn existing_podman_socket() -> Option<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", current_uid())));
    let socket_path = runtime_dir.join("podman/podman.sock");

    path_is_socket(&socket_path)
        .ok()
        .filter(|is_socket| *is_socket)?;
    Some(socket_path)
}

fn wait_for_podman_socket() -> Option<PathBuf> {
    for _ in 0..25 {
        if let Some(socket) = existing_podman_socket() {
            return Some(socket);
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    None
}

fn link_managed_socket(
    managed_socket: &Path,
    target_socket: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if path_is_socket(managed_socket)? {
        return Ok(());
    }

    if fs::symlink_metadata(managed_socket).is_ok() {
        fs::remove_file(managed_socket)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target_socket, managed_socket)?;
    }

    #[cfg(not(unix))]
    {
        return Err(Box::new(std::io::Error::other(
            "podman fallback symlink is only supported on unix",
        )));
    }

    Ok(())
}

fn start_podman_service(
    managed_socket: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if fs::symlink_metadata(managed_socket).is_ok() {
        fs::remove_file(managed_socket)?;
    }

    let log_path = managed_socket.with_extension("log");
    let log_file = fs::File::create(&log_path)?;
    let log_file_err = log_file.try_clone()?;
    let socket_spec = format!("unix://{}", managed_socket.display());

    let child = Command::new("podman")
        .args(["system", "service", "--time=600", socket_spec.as_str()])
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()?;
    std::mem::forget(child);

    for _ in 0..50 {
        if path_is_socket(managed_socket)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let log_output = fs::read_to_string(&log_path).unwrap_or_default();
    Err(Box::new(std::io::Error::other(format!(
        "podman socket did not become available at {}{}{}",
        managed_socket.display(),
        if log_output.is_empty() { "" } else { "; log: " },
        log_output
    ))))
}

fn current_uid() -> String {
    std::env::var("UID").unwrap_or_else(|_| {
        Command::new("id")
            .arg("-u")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "0".to_string())
    })
}

fn path_is_socket(path: &Path) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Ok(false);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;

        Ok(fs::metadata(path)?.file_type().is_socket())
    }

    #[cfg(not(unix))]
    {
        Ok(true)
    }
}
