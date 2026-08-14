use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

use tempfile::TempDir;

pub struct TlsMaterial {
    dir: TempDir,
    pub ca_cert: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
    ca_cert_pem: String,
    client_cert_pem: String,
    client_key_pem: String,
}

impl TlsMaterial {
    pub fn new() -> io::Result<Self> {
        ensure_crypto_provider();
        let dir = tempfile::tempdir()?;
        let ca_key = dir.path().join("ca.key");
        let ca_cert = dir.path().join("ca.crt");
        let server_key = dir.path().join("server.key");
        let server_csr = dir.path().join("server.csr");
        let server_ext = dir.path().join("server.ext");
        let server_cert = dir.path().join("server.crt");
        let client_key = dir.path().join("client.key");
        let client_csr = dir.path().join("client.csr");
        let client_ext = dir.path().join("client.ext");
        let client_cert = dir.path().join("client.crt");

        run_openssl([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-days",
            "3650",
            "-nodes",
            "-keyout",
            &path_string(&ca_key)?,
            "-out",
            &path_string(&ca_cert)?,
            "-subj",
            "/CN=rehydration-test-ca",
        ])?;

        run_openssl([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            &path_string(&server_key)?,
            "-out",
            &path_string(&server_csr)?,
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
        ])?;
        fs::write(
            &server_ext,
            "[v3_req]\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n",
        )?;
        run_openssl([
            "x509",
            "-req",
            "-in",
            &path_string(&server_csr)?,
            "-CA",
            &path_string(&ca_cert)?,
            "-CAkey",
            &path_string(&ca_key)?,
            "-CAcreateserial",
            "-out",
            &path_string(&server_cert)?,
            "-days",
            "3650",
            "-extfile",
            &path_string(&server_ext)?,
            "-extensions",
            "v3_req",
        ])?;

        run_openssl([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            &path_string(&client_key)?,
            "-out",
            &path_string(&client_csr)?,
            "-subj",
            "/CN=rehydration-test-client",
        ])?;
        fs::write(&client_ext, "[v3_req]\nextendedKeyUsage=clientAuth\n")?;
        run_openssl([
            "x509",
            "-req",
            "-in",
            &path_string(&client_csr)?,
            "-CA",
            &path_string(&ca_cert)?,
            "-CAkey",
            &path_string(&ca_key)?,
            "-CAcreateserial",
            "-out",
            &path_string(&client_cert)?,
            "-days",
            "3650",
            "-extfile",
            &path_string(&client_ext)?,
            "-extensions",
            "v3_req",
        ])?;

        Ok(Self {
            ca_cert_pem: fs::read_to_string(&ca_cert)?,
            client_cert_pem: fs::read_to_string(&client_cert)?,
            client_key_pem: fs::read_to_string(&client_key)?,
            dir,
            ca_cert,
            server_cert,
            server_key,
            client_cert,
            client_key,
        })
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn ca_certificate_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    pub fn client_identity_pem(&self) -> (&str, &str) {
        (&self.client_cert_pem, &self.client_key_pem)
    }
}

pub fn ensure_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn path_string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| io::Error::other(format!("path is not valid UTF-8: {}", path.display())))
}

fn run_openssl<const N: usize>(args: [&str; N]) -> io::Result<()> {
    let output = Command::new("openssl").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "openssl command failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}
