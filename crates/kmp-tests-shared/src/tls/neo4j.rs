use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::Duration;

use kmp_testkit::ensure_testcontainers_runtime;
use neo4rs::{ConfigBuilder, Graph, query};
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor, logs::LogFrame},
    runners::AsyncRunner,
};
use tokio::time::{sleep, timeout};

use crate::containers::NEO4J_PASSWORD;
use crate::debug::debug_enabled;
use crate::tls::material::{TlsMaterial, ensure_crypto_provider};

const NEO4J_IMAGE: &str = "docker.io/library/neo4j";
const NEO4J_TAG: &str = "5.26.0-community";
const NEO4J_INTERNAL_PORT: u16 = 7687;
const NEO4J_STARTUP_WAIT: Duration = Duration::from_secs(10);
const NEO4J_CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn start_neo4j_tls_container(
    tls_material: &TlsMaterial,
) -> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    ensure_testcontainers_runtime()?;
    let ssl_dir = prepare_neo4j_ssl_directory(tls_material)?;

    let image = GenericImage::new(NEO4J_IMAGE, NEO4J_TAG)
        .with_exposed_port(NEO4J_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::seconds(NEO4J_STARTUP_WAIT.as_secs()))
        .with_env_var("NEO4J_AUTH", format!("neo4j/{NEO4J_PASSWORD}"))
        .with_env_var("NEO4J_server_bolt_tls__level", "REQUIRED")
        .with_env_var("NEO4J_dbms_ssl_policy_bolt_enabled", "true")
        .with_env_var("NEO4J_dbms_ssl_policy_bolt_base__directory", "/ssl")
        .with_env_var("NEO4J_dbms_ssl_policy_bolt_private__key", "private.key")
        .with_env_var(
            "NEO4J_dbms_ssl_policy_bolt_public__certificate",
            "public.crt",
        )
        .with_env_var("NEO4J_dbms_ssl_policy_bolt_client__auth", "NONE")
        .with_copy_to("/ssl", ssl_dir);

    let image = if debug_enabled() {
        image.with_log_consumer(|frame: &LogFrame| {
            eprintln!(
                "[neo4j-tls] {}",
                String::from_utf8_lossy(frame.bytes()).trim_end()
            );
        })
    } else {
        image
    };

    Ok(image.start().await?)
}

pub async fn clear_neo4j_tls(
    uri: String,
    ca_path: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let graph = connect_with_retry(uri, "neo4j", NEO4J_PASSWORD, ca_path).await?;
    graph.run(query("MATCH (n) DETACH DELETE n")).await?;
    Ok(())
}

async fn connect_with_retry(
    uri: String,
    user: &str,
    password: &str,
    ca_path: &Path,
) -> Result<Graph, Box<dyn Error + Send + Sync>> {
    timeout(
        CONNECT_TIMEOUT,
        connect_with_retry_inner(uri, user.to_string(), password.to_string(), ca_path),
    )
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "neo4j TLS connection did not become ready within {:?}",
                CONNECT_TIMEOUT
            ),
        )
    })?
}

async fn connect_with_retry_inner(
    uri: String,
    user: String,
    password: String,
    ca_path: &Path,
) -> Result<Graph, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;

    for _ in 0..NEO4J_CONNECT_RETRY_ATTEMPTS {
        match connect_tls_graph(&uri, &user, &password, ca_path).await {
            Ok(graph) => return Ok(graph),
            Err(error) => {
                last_error = Some(error);
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}

async fn connect_tls_graph(
    uri: &str,
    user: &str,
    password: &str,
    ca_path: &Path,
) -> Result<Graph, Box<dyn Error + Send + Sync>> {
    ensure_crypto_provider();
    let config = ConfigBuilder::new()
        .uri(uri)
        .user(user)
        .password(password)
        .with_client_certificate(ca_path)
        .build()?;

    Graph::connect(config)
        .await
        .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)
}

fn prepare_neo4j_ssl_directory(
    tls_material: &TlsMaterial,
) -> Result<std::path::PathBuf, Box<dyn Error + Send + Sync>> {
    let ssl_dir = tls_material.dir().join("neo4j-ssl");
    let trusted_dir = ssl_dir.join("trusted");
    let revoked_dir = ssl_dir.join("revoked");

    fs::create_dir_all(&trusted_dir)?;
    fs::create_dir_all(&revoked_dir)?;
    fs::copy(&tls_material.server_cert, ssl_dir.join("public.crt"))?;
    fs::copy(&tls_material.server_key, ssl_dir.join("private.key"))?;
    fs::copy(&tls_material.ca_cert, trusted_dir.join("ca.crt"))?;
    set_directory_permissions(&ssl_dir)?;
    set_directory_permissions(&trusted_dir)?;
    set_directory_permissions(&revoked_dir)?;
    set_file_permissions(&ssl_dir.join("public.crt"))?;
    set_file_permissions(&ssl_dir.join("private.key"))?;
    set_file_permissions(&trusted_dir.join("ca.crt"))?;

    Ok(ssl_dir)
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    Ok(())
}
