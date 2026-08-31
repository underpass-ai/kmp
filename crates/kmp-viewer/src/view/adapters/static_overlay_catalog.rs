//! The overlay vocabulary mounted beside this registry.

use std::collections::BTreeSet;
use std::sync::Mutex;

use crate::view::domain::OverlayName;
use crate::view::ports::OverlayCatalog;

/// The set of telemetry series the mounted viewer reader resolves, published
/// once at composition and replaced wholesale if the reader changes.
#[derive(Default)]
pub struct StaticOverlayCatalog {
    names: Mutex<BTreeSet<String>>,
}

impl StaticOverlayCatalog {
    /// An empty catalog: no overlay resolves until one is published.
    pub fn new() -> Self {
        Self::default()
    }
}

impl OverlayCatalog for StaticOverlayCatalog {
    fn publish(&self, names: Vec<OverlayName>) {
        *self.names.lock().expect("overlay catalog poisoned") = names
            .into_iter()
            .map(|name| name.as_str().to_string())
            .collect();
    }

    fn contains(&self, name: &OverlayName) -> bool {
        self.names
            .lock()
            .expect("overlay catalog poisoned")
            .contains(name.as_str())
    }
}
