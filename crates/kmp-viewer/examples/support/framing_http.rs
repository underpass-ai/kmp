use kmp_embedded::EmbeddedKernel;
use kmp_viewer::{MemoryViewerServer, bind_loopback};
use std::{error::Error, sync::Arc, time::Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct FramingHttp {
    port: u16,
    cookie: String,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Drop for FramingHttp {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl FramingHttp {
    pub async fn open(kernel: &EmbeddedKernel) -> Result<Self, Box<dyn Error>> {
        let viewer = Arc::new(MemoryViewerServer::new(kernel.service(), None)?);
        let listener = bind_loopback("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let origin = format!("http://127.0.0.1:{port}");
        let invitation = viewer.capability_url(&format!("{origin}/"));
        let server = tokio::spawn(viewer.serve(listener));
        let mut probe = Self {
            port,
            cookie: String::new(),
            server,
        };
        let raw = probe
            .raw(invitation.strip_prefix(&origin).expect("local invitation"))
            .await?;
        probe.cookie = raw
            .lines()
            .find_map(|s| s.strip_prefix("Set-Cookie: "))
            .and_then(|s| s.split(';').next())
            .expect("bootstrap cookie")
            .to_string();
        Ok(probe)
    }
    async fn raw(&self, path: &str) -> Result<String, Box<dyn Error>> {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", self.port)).await?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
            self.port, self.cookie
        );
        stream.write_all(request.as_bytes()).await?;
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await?;
        Ok(raw)
    }
    pub async fn measure(
        &self,
        baseline: bool,
        about: &str,
        refs: &[String],
    ) -> Result<(u128, usize), Box<dyn Error>> {
        // Synthetic refs contain only alphanumeric, colon and hyphen characters.
        let paths = if baseline {
            refs.iter()
                .map(|r| format!("/api/node?about={about}&id={r}&raw=1"))
                .collect()
        } else {
            vec![format!(
                "/api/nodes?about={about}&ids={}&max_edges=32768",
                refs.join(",")
            )]
        };
        let start = Instant::now();
        let mut bytes = 0;
        for path in paths {
            let raw = self.raw(&path).await?;
            assert!(raw.starts_with("HTTP/1.1 200"), "read failed");
            bytes += raw.len();
            let body: serde_json::Value =
                serde_json::from_str(raw.split_once("\r\n\r\n").expect("HTTP body").1)?;
            if !baseline {
                assert!(body["omitted"].as_array().expect("omitted").is_empty());
            }
        }
        Ok((start.elapsed().as_micros(), bytes))
    }
}
