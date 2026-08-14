#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmpApplication;

impl KmpApplication {
    pub const fn capability_name() -> &'static str {
        "deterministic-context-rehydration"
    }
}
