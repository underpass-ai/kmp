use serde_json::Value;

use crate::domain::plugin_package_version::PluginPackageVersion;
use crate::domain::release_error::ReleaseError;

pub struct PluginManifestPackageMapper;

impl PluginManifestPackageMapper {
    pub fn stamp(content: &[u8], version: &PluginPackageVersion) -> Result<Vec<u8>, ReleaseError> {
        let mut body: Value = serde_json::from_slice(content).map_err(|error| {
            ReleaseError::invalid(format!("plugin manifest is invalid: {error}"))
        })?;
        if !body.is_object() {
            return Err(ReleaseError::invalid("plugin manifest must be an object"));
        }
        body["version"] = Value::String(version.as_str().to_string());
        serde_json::to_vec_pretty(&body)
            .map(|mut bytes| {
                bytes.push(b'\n');
                bytes
            })
            .map_err(|error| {
                ReleaseError::invalid(format!("cannot encode plugin manifest: {error}"))
            })
    }
}
