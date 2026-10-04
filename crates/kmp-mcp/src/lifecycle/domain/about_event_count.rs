/// One about a store holds, and how many events were written under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutEventCount {
    about: String,
    events: u64,
}

impl AboutEventCount {
    pub fn new(about: impl Into<String>, events: u64) -> Self {
        Self {
            about: about.into(),
            events,
        }
    }

    /// The about exactly as stored; never normalized.
    pub fn about(&self) -> &str {
        &self.about
    }

    pub fn events(&self) -> u64 {
        self.events
    }
}
