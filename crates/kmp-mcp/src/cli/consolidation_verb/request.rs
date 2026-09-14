use super::{
    project_request::ProjectRequest, read_request::ReadRequest, source_request::SourceRequest,
    validation,
};
use kmp_domain::consolidation::ConsolidationWrite;

pub(super) enum Request {
    Sources(SourceRequest),
    Write(ConsolidationWrite),
    Read(ReadRequest),
    Project(ProjectRequest),
}

impl Request {
    /// Validate intrinsic request semantics before opening a store. Source
    /// existence, quote grounding and revision conflicts remain store checks.
    pub fn parse(operation: &str, bytes: &[u8]) -> Result<Self, String> {
        let request = match operation {
            "sources" => Self::Sources(serde_json::from_slice(bytes).map_err(|e| e.to_string())?),
            "write" => Self::Write(serde_json::from_slice(bytes).map_err(|e| e.to_string())?),
            "read" => Self::Read(serde_json::from_slice(bytes).map_err(|e| e.to_string())?),
            "project" => Self::Project(serde_json::from_slice(bytes).map_err(|e| e.to_string())?),
            _ => return Err("expected sources, write, read or project".into()),
        };
        match &request {
            Self::Sources(source) => {
                validation::identifier("about", &source.about)?;
                validation::source_refs(source.refs.iter().map(String::as_str))?;
            }
            Self::Write(write) => validation::write(write)?,
            Self::Read(read) => {
                validation::identifier("about", &read.about)?;
                validation::identifier("view", &read.view)?;
                if read.revision == Some(0) {
                    return Err("revision must be greater than zero".into());
                }
            }
            Self::Project(project) => {
                validation::identifier("about", &project.about)?;
                validation::identifier("view", &project.view)?;
                if !(512..=1_048_576).contains(&project.max_bytes.unwrap_or(10000)) {
                    return Err("projection max_bytes must be 512..1048576".into());
                }
                if let Some(selection) = &project.selection {
                    selection.validate().map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(request)
    }
}
