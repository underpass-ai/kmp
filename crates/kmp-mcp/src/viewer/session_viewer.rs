//! The viewer one stdio session holds: bound on the first request for it.

use std::sync::OnceLock;

use kmp_viewer::VIEWER_ADDR_ENV;

use super::{ViewerAddr, ViewerMount};

/// The viewer as one session holds it. Nothing listens until a request needs
/// the viewer; the first one binds it and every later one gets the same
/// listener and link for the rest of the session.
pub struct SessionViewer {
    addr: ViewerAddr,
    mount: Option<ViewerMount>,
    url: OnceLock<String>,
    starting: tokio::sync::Mutex<()>,
}

impl SessionViewer {
    /// A viewer that binds `addr` the first time it is asked for.
    pub fn on_demand(addr: ViewerAddr, mount: ViewerMount) -> Self {
        Self {
            addr,
            mount: Some(mount),
            url: OnceLock::new(),
            starting: tokio::sync::Mutex::new(()),
        }
    }

    /// A viewer some other part of the process already serves at `url`.
    pub fn already_serving(url: impl Into<String>) -> Self {
        let served = OnceLock::new();
        let _ = served.set(url.into());
        Self {
            addr: ViewerAddr::Off,
            mount: None,
            url: served,
            starting: tokio::sync::Mutex::new(()),
        }
    }

    /// The link, when the viewer is already listening. Asking here never
    /// starts it.
    pub fn url(&self) -> Option<&str> {
        self.url.get().map(String::as_str)
    }

    /// The link, binding the listener first when nothing listens yet. A
    /// failure names the address and the reason; nothing is cached, so a
    /// later request tries again.
    pub async fn ensure(&self) -> Result<&str, String> {
        if let Some(url) = self.url() {
            return Ok(url);
        }
        let _turn = self.starting.lock().await;
        if let Some(url) = self.url() {
            return Ok(url);
        }
        let (Some(mount), Some(addr)) = (self.mount.as_ref(), self.addr.addr()) else {
            return Err(format!(
                "{VIEWER_ADDR_ENV} declines the viewer in this session"
            ));
        };
        let url = match mount.serve_at(addr).await {
            Ok(url) => url,
            // The default address is only a preference: if another session
            // owns it, this one takes a free loopback port of its own.
            Err(message) if !self.addr.was_asked_for() => {
                eprintln!("kmp-mcp: {message}; choosing a free per-session loopback port instead");
                mount.serve_at("127.0.0.1:0").await.map_err(|fallback| {
                    format!("{message}; a free loopback port could not be bound either: {fallback}")
                })?
            }
            // An address the operator named is a contract.
            Err(message) => {
                return Err(format!(
                    "{message}. Usually another session already has it: name a free port with \
                     {VIEWER_ADDR_ENV}=127.0.0.1:7318, or {VIEWER_ADDR_ENV}=off to go without"
                ));
            }
        };
        Ok(self.url.get_or_init(|| url))
    }
}
