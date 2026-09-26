//! Test support: the JSON lines a subscriber writes, captured in memory.

use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

/// JSON log lines written while its guard is held, filtered like the server
/// filters (`RUST_LOG` syntax), on this thread only.
#[derive(Clone, Default)]
pub(crate) struct CapturedLog {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLog {
    /// Capture with `filter` until the returned guard drops.
    pub(crate) fn start(filter: &str) -> (Self, DefaultGuard) {
        let log = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::new(filter))
            .with_writer(log.clone())
            .finish();
        (log, tracing::subscriber::set_default(subscriber))
    }

    /// Every captured line whose `fields.event` is `event`.
    pub(crate) fn events(&self, event: &str) -> Vec<Value> {
        let bytes = self.bytes.lock().expect("log").clone();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|line| line["fields"]["event"] == event)
            .collect()
    }
}

impl Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().expect("log").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedLog {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
