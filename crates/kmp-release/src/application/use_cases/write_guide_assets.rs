use crate::application::use_cases::prepare_guide_requests::PrepareGuideRequests;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::guide_engine::GuideEngine;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct WriteGuideAssets<'a, F, G: ?Sized> {
    file_system: &'a F,
    engine: &'a G,
}

impl<'a, F, G> WriteGuideAssets<'a, F, G>
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
        let requests = PrepareGuideRequests::new(self.file_system, self.engine).execute(root)?;
        let plugin = root.join("plugins/kmp");
        self.file_system.write_text(
            &plugin.join("guide/guide.requests.json"),
            &PrepareGuideRequests::<F, G>::text(&requests)?,
        )?;
        let scratch = root.join(format!("tmp/guide-build-{}", std::process::id()));
        self.file_system.remove_dir_all_if_present(&scratch)?;
        self.file_system.create_dir_all(&scratch)?;
        let generated = scratch.join("memory.jsonl");
        let build_result = (|| {
            self.engine.ingest(&requests, Some(&scratch))?;
            self.engine.export(&scratch, &generated)?;
            let bundle = self.file_system.read_bytes(&generated)?;
            self.file_system
                .write_bytes(&plugin.join("guide/memory.jsonl"), &bundle)
        })();
        let cleanup_result = self.file_system.remove_dir_all_if_present(&scratch);
        build_result?;
        cleanup_result
    }
}
