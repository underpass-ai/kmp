//! The viewer's server over one embedded kernel, ready to bind on request.

use std::sync::Arc;

use kmp_application::ObservabilityQueryPort;

/// What it takes to serve ChronoLoom over one kernel, kept until someone
/// asks for the viewer. The listener belongs to that moment, not to the
/// session's start.
pub struct ViewerMount {
    service: Arc<kmp_embedded::EmbeddedMemoryService>,
    data_dir: String,
    observability: Option<Arc<dyn ObservabilityQueryPort>>,
    observability_unavailable: Option<String>,
}

impl ViewerMount {
    /// The viewer over this kernel's live facade: no second store
    /// connection, no read model of its own.
    pub fn over(kernel: &kmp_embedded::EmbeddedKernel) -> Self {
        Self {
            service: kernel.service(),
            data_dir: kernel.data_dir().display().to_string(),
            observability: kernel
                .quality_telemetry_reader()
                .map(|reader| Arc::new(reader) as Arc<dyn ObservabilityQueryPort>),
            observability_unavailable: kernel.quality_telemetry_error().map(str::to_string),
        }
    }

    /// Binds `addr` (loopback only), serves the viewer in the background for
    /// the life of this process and returns the capability link a human
    /// opens. The link names the bound address, not the requested one: `:0`
    /// resolves here.
    pub async fn serve_at(&self, addr: &str) -> Result<String, String> {
        let mut viewer = kmp_viewer::MemoryViewerServer::new(
            Arc::clone(&self.service),
            Some(self.data_dir.clone()),
        )
        .map_err(|error| format!("viewer capability could not be created: {error}"))?;
        if let Some(reader) = &self.observability {
            viewer = viewer.with_observability(Arc::clone(reader));
        } else if let Some(reason) = self.observability_unavailable.as_deref() {
            viewer = viewer.with_observability_unavailable(reason);
        }
        let viewer = Arc::new(viewer);
        let listener = kmp_viewer::bind_loopback(addr)
            .await
            .map_err(|error| format!("viewer could not bind `{addr}`: {error}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| format!("viewer listener has no local address: {error}"))?;
        let url = viewer.capability_url(&format!("http://{local_addr}/"));
        if let Some(reason) = &self.observability_unavailable {
            eprintln!("kmp-mcp: memory viewer at {url}; observability unavailable: {reason}");
        } else {
            eprintln!("kmp-mcp: memory viewer at {url}");
        }
        tokio::spawn(async move {
            if let Err(error) = viewer.serve(listener).await {
                tracing::error!(%error, "memory viewer stopped");
            }
        });
        Ok(url)
    }
}
