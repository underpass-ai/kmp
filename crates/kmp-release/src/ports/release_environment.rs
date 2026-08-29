pub trait ReleaseEnvironment {
    fn value(&self, name: &str) -> Option<String>;
}
