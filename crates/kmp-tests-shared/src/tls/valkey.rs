use std::error::Error;
use std::fs;

use kmp_testkit::ensure_testcontainers_runtime;
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::tls::material::TlsMaterial;

pub const VALKEY_INTERNAL_PORT: u16 = 6379;

pub async fn start_valkey_tls_container(
    tls_material: &TlsMaterial,
) -> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    ensure_testcontainers_runtime()?;
    fs::write(
        tls_material.dir().join("valkey.conf"),
        r#"port 0
bind 0.0.0.0
protected-mode no
save ""
appendonly no
tls-port 6379
tls-cert-file /tls/server.crt
tls-key-file /tls/server.key
tls-ca-cert-file /tls/ca.crt
tls-auth-clients yes
"#,
    )?;

    Ok(GenericImage::new("docker.io/valkey/valkey", "8.1.5-alpine")
        .with_entrypoint("valkey-server")
        .with_exposed_port(VALKEY_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .with_copy_to("/tls", tls_material.dir())
        .with_cmd(vec!["/tls/valkey.conf"])
        .start()
        .await?)
}
