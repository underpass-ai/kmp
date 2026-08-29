use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideBundleHeaderDto {
    pub bundle_format: u32,
    pub event_count: u64,
    pub kernel_version: String,
    pub abouts: Vec<String>,
}
