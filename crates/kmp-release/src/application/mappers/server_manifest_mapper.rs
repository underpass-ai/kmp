use serde_json::Value;

use crate::application::dto::server_manifest_document_dto::ServerManifestDocumentDto;
use crate::domain::mcpb_digest::McpbDigest;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;

pub struct ServerManifestMapper;

impl ServerManifestMapper {
    pub fn parse(text: &str) -> Result<ServerManifestDocumentDto, ReleaseError> {
        let body: Value = serde_json::from_str(text)
            .map_err(|error| ReleaseError::invalid(format!("server.json is invalid: {error}")))?;
        if !body.is_object() {
            return Err(ReleaseError::invalid("server.json must be an object"));
        }
        Ok(ServerManifestDocumentDto { body })
    }

    pub fn stamp(
        document: &mut ServerManifestDocumentDto,
        version: &ReleaseVersion,
        digest: &McpbDigest,
    ) -> Result<String, ReleaseError> {
        if document.body["version"].as_str() != Some(version.as_str()) {
            return Err(ReleaseError::invalid(format!(
                "server.json version does not match workspace {version}"
            )));
        }
        let packages = document.body["packages"]
            .as_array_mut()
            .ok_or_else(|| ReleaseError::invalid("server.json packages must be an array"))?;
        let mut mcpb = packages
            .iter_mut()
            .filter(|package| package["registryType"] == "mcpb")
            .collect::<Vec<_>>();
        if mcpb.len() != 1 {
            return Err(ReleaseError::invalid(format!(
                "server.json must contain exactly one MCPB package, found {}",
                mcpb.len()
            )));
        }
        let identifier = format!(
            "https://github.com/underpass-ai/kmp/releases/download/{}/kmp-mcp-{}.mcpb",
            version.tag(),
            version.tag()
        );
        mcpb[0]["identifier"] = Value::String(identifier.clone());
        mcpb[0]["fileSha256"] = Value::String(digest.as_str().to_string());
        Ok(identifier)
    }

    pub fn text(document: &ServerManifestDocumentDto) -> Result<String, ReleaseError> {
        serde_json::to_string_pretty(&document.body)
            .map(|text| format!("{text}\n"))
            .map_err(|error| ReleaseError::invalid(format!("cannot encode server.json: {error}")))
    }
}
