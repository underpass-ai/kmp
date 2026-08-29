use crate::ports::release_environment::ReleaseEnvironment;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemEnvironment;

impl ReleaseEnvironment for SystemEnvironment {
    fn value(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}
