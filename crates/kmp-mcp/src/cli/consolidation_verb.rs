//! JSON is a CLI boundary only. Domain policy and atomic persistence live in
//! the kernel; no model or remote service is invoked here.
use kmp_application::consolidation::ConsolidationApplicationService;
use serde_json::Value;
use std::{io::Read, sync::Arc};

mod project_request;
mod read_request;
mod request;
mod source_input;
mod source_request;
mod validation;
mod write_receipt;
use request::Request;

pub(super) async fn run(args: &[&str]) -> i32 {
    match execute(args).await {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp consolidation: {error}");
            2
        }
    }
}

async fn execute(args: &[&str]) -> Result<Value, String> {
    let (operation, path) = match args {
        [operation] => (*operation, "-"),
        [operation, path] if *path == "-" || !path.starts_with('-') => (*operation, *path),
        _ => return Err("usage: kmp-mcp consolidation sources|write|read|project [FILE|-]".into()),
    };
    if !matches!(operation, "sources" | "write" | "read" | "project") {
        return Err("expected sources, write, read or project".into());
    }
    let input: Box<dyn Read> = if path == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|e| e.to_string())?)
    };
    let mut bytes = Vec::new();
    input
        .take(2_097_153)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2_097_152 {
        return Err("input exceeds 2 MiB".into());
    }
    // Validate the entire request before opening or creating a store.
    let request = Request::parse(operation, &bytes)?;
    let resolved = kmp_embedded::resolve_data_dir_from_env().map_err(|e| e.to_string())?;
    let store =
        kmp_embedded::EmbeddedKernelStore::open(resolved.path()).map_err(|e| e.to_string())?;
    let service = ConsolidationApplicationService::new(Arc::new(store));
    let result = match request {
        Request::Sources(request) => {
            let sources = service
                .sources(request.about, request.refs)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(
                sources
                    .iter()
                    .map(source_input::SourceInput::from)
                    .collect::<Vec<_>>(),
            )
        }
        Request::Write(request) => {
            let accepted = service.write(request).await.map_err(|e| e.to_string())?;
            serde_json::to_value(write_receipt::WriteReceipt::from(&accepted))
        }
        Request::Read(request) => serde_json::to_value(
            service
                .read(request.about, request.view, request.revision)
                .await
                .map_err(|e| e.to_string())?,
        ),
        Request::Project(request) => {
            let read = service
                .read(request.about, request.view, None)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(
                kmp_proto_mapping::consolidation_projection::project(
                    &read,
                    request.selection,
                    request.max_bytes.unwrap_or(10000),
                )
                .map_err(|e| e.to_string())?,
            )
        }
    };
    result.map_err(|e| e.to_string())
}
