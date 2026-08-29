#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarketplaceSource;

impl MarketplaceSource {
    pub const fn name(self) -> &'static str {
        "underpass"
    }

    pub const fn repository(self) -> &'static str {
        "underpass-ai/kmp"
    }

    pub const fn distribution_ref(self) -> &'static str {
        "marketplace"
    }

    pub fn claude_locator(self) -> String {
        format!("{}@{}", self.repository(), self.distribution_ref())
    }
}
