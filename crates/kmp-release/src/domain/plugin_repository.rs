/// The clone URL Claude Code is pointed at. It is a release contract rather
/// than a caller's choice — the catalog is checked against it — so it lives in
/// one place and every default reads from here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PluginRepository;

impl PluginRepository {
    pub const URL: &'static str = "https://github.com/underpass-ai/kmp.git";
}
