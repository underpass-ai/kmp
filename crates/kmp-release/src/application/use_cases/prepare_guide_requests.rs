use crate::application::dto::guide_capabilities_dto::GuideCapabilitiesDto;
use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::application::dto::guide_source_dto::GuideSourceDto;
use crate::application::mappers::guide_request_mapper::GuideRequestMapper;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::guide_engine::GuideEngine;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct PrepareGuideRequests<'a, F, G: ?Sized> {
    file_system: &'a F,
    engine: &'a G,
}

impl<'a, F, G> PrepareGuideRequests<'a, F, G>
where
    F: ReleaseFileSystem + CandidateFileSystem,
    G: GuideEngine + ?Sized,
{
    pub fn new(file_system: &'a F, engine: &'a G) -> Self {
        Self {
            file_system,
            engine,
        }
    }

    pub fn execute(
        &self,
        root: &RepositoryRoot,
    ) -> Result<Vec<GuideRequestDocumentDto>, ReleaseError> {
        let plugin = root.join("plugins/kmp");
        let source: GuideSourceDto = serde_json::from_str(
            &self
                .file_system
                .read_text(&plugin.join("guide/editorial.json"))?,
        )
        .map_err(|error| ReleaseError::invalid(format!("editorial.json is invalid: {error}")))?;
        let capabilities: GuideCapabilitiesDto = serde_json::from_str(
            &self
                .file_system
                .read_text(&plugin.join("capabilities.json"))?,
        )
        .map_err(|error| ReleaseError::invalid(format!("capabilities.json is invalid: {error}")))?;
        let scratch = root.join(format!("tmp/guide-tools-{}", std::process::id()));
        self.file_system.remove_dir_all_if_present(&scratch)?;
        self.file_system.create_dir_all(&scratch)?;
        let tools_result = self.engine.live_tools(&scratch);
        let cleanup_result = self.file_system.remove_dir_all_if_present(&scratch);
        let tools = tools_result?;
        cleanup_result?;
        let names = tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>();
        let mut expected = capabilities.mcp_tools;
        expected.sort();
        if names != expected {
            return Err(ReleaseError::invalid(format!(
                "live tools differ from capabilities.json: live={names:?}, expected={expected:?}"
            )));
        }
        GuideRequestMapper::map(&source, &tools)
    }

    pub fn text(requests: &[GuideRequestDocumentDto]) -> Result<String, ReleaseError> {
        let bodies = requests
            .iter()
            .map(|request| request.body.clone())
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&bodies)
            .map(|text| format!("{text}\n"))
            .map_err(|error| {
                ReleaseError::invalid(format!("could not encode guide requests: {error}"))
            })
    }
}
