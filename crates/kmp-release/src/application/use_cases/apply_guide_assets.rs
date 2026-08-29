use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::application::use_cases::prepare_guide_requests::PrepareGuideRequests;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::guide_engine::GuideEngine;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct ApplyGuideAssets<'a, F, G: ?Sized> {
    file_system: &'a F,
    engine: &'a G,
}

impl<'a, F, G> ApplyGuideAssets<'a, F, G>
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

    pub fn execute(&self, root: &RepositoryRoot) -> Result<(), ReleaseError> {
        let expected = PrepareGuideRequests::new(self.file_system, self.engine).execute(root)?;
        let path = root.join("plugins/kmp/guide/guide.requests.json");
        let actual: Vec<serde_json::Value> =
            serde_json::from_str(&self.file_system.read_text(&path)?).map_err(|error| {
                ReleaseError::invalid(format!("{} is invalid: {error}", path.display()))
            })?;
        let requests = actual
            .into_iter()
            .map(|body| GuideRequestDocumentDto { body })
            .collect::<Vec<_>>();
        if PrepareGuideRequests::<F, G>::text(&requests)?
            != PrepareGuideRequests::<F, G>::text(&expected)?
        {
            return Err(ReleaseError::invalid(
                "installed guide requests do not match this engine's live tool surface",
            ));
        }
        self.engine.ingest(&requests, None)
    }
}
