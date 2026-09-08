//! Apply the two shipped guide documents through the ordinary MCP writer.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub const ABOUTS: [&str; 2] = ["guide:kmp", "guide:kmp-agent"];

pub fn abouts_owned() -> Vec<String> {
    ABOUTS.map(str::to_string).to_vec()
}

pub fn is_guide_about(about: &str) -> bool {
    ABOUTS.contains(&about)
}

pub async fn sync(arguments: &[&str]) -> Result<String, String> {
    let (root, dry_run) = parse_arguments(arguments)?;
    let requests = load_assets(&root)?;
    if !dry_run {
        let server = crate::KernelMcpServer::try_from_env()?;
        for (offset, request) in requests.iter().enumerate() {
            let call = json!({
                "jsonrpc": "2.0", "id": offset + 1, "method": "tools/call",
                "params": {"name": "kmp_ingest", "arguments": request}
            });
            let response = server
                .handle_json_line(&call.to_string())
                .await
                .ok_or_else(|| "guide ingest returned no MCP response".to_string())?;
            let response: Value = serde_json::from_str(&response)
                .map_err(|error| format!("guide ingest returned invalid JSON: {error}"))?;
            if response.get("error").is_some() || response["result"]["isError"] != false {
                return Err(format!(
                    "guide ingest failed for {}: {response}",
                    request["about"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    let action = if dry_run {
        "would converge"
    } else {
        "converged"
    };
    Ok(format!(
        "KMP guide: {action} {} immutable guide memories into the selected store",
        requests.len()
    ))
}

fn parse_arguments(arguments: &[&str]) -> Result<(PathBuf, bool), String> {
    if arguments.first().copied() != Some("sync") {
        return Err("usage: kmp-mcp guide sync --plugin-root DIR [--dry-run]".into());
    }
    let mut root = None;
    let mut dry_run = false;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index] {
            "--plugin-root" => {
                index += 1;
                root = Some(PathBuf::from(
                    arguments
                        .get(index)
                        .ok_or("--plugin-root needs a directory")?,
                ));
            }
            "--dry-run" => dry_run = true,
            option => return Err(format!("unknown guide option `{option}`")),
        }
        index += 1;
    }
    let root = root.ok_or("guide sync needs --plugin-root DIR")?;
    if root.as_os_str().is_empty() {
        return Err("guide plugin root cannot be empty".into());
    }
    Ok((root, dry_run))
}

fn load_assets(root: &Path) -> Result<Vec<Value>, String> {
    let path = root.join("guide/guide.requests.json");
    let text = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "could not read guide requests `{}`: {error}",
            path.display()
        )
    })?;
    let requests: Vec<Value> = serde_json::from_str(&text)
        .map_err(|error| format!("guide requests `{}` are invalid: {error}", path.display()))?;
    for request in &requests {
        if !request["about"].as_str().is_some_and(is_guide_about) {
            return Err("guide request has an unsupported about".into());
        }
        if request["idempotency_key"]
            .as_str()
            .is_none_or(str::is_empty)
        {
            return Err("guide request has no idempotency key".into());
        }
        if !request["memory"].is_object() {
            return Err("guide request has no memory document".into());
        }
    }
    let abouts = requests
        .iter()
        .filter_map(|r| r["about"].as_str())
        .collect::<BTreeSet<_>>();
    if requests.len() != 2 || abouts != BTreeSet::from(ABOUTS) {
        return Err("guide requests must contain exactly guide:kmp and guide:kmp-agent".into());
    }
    let path = root.join("guide/memory.jsonl");
    let bundle = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read guide bundle `{}`: {error}", path.display()))?;
    let header = kmp_embedded::verify_bundle(&bundle)
        .map_err(|error| format!("guide bundle is invalid: {error}"))?;
    if header.event_count != 2 || header.abouts != abouts_owned() {
        return Err("guide bundle must contain exactly the two guide memories".into());
    }
    Ok(requests)
}
