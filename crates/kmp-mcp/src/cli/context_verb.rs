//! Pure context composition over captured packets; no backend or store opens.
use kmp_proto_mapping::context_projection::{ProjectionRequest, expand};
use serde_json::Value;
use std::io::Read;

pub(super) fn run(args: &[&str]) -> i32 {
    match project(args) {
        Ok(result) => {
            println!("{result}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp context: {error}");
            2
        }
    }
}

fn project(args: &[&str]) -> Result<Value, String> {
    let (operation, path) = match args {
        [operation] => (*operation, "-"),
        [operation, path] if *path == "-" || !path.starts_with('-') => (*operation, *path),
        _ => return Err("usage: kmp-mcp context project|expand [FILE|-]".into()),
    };
    if !matches!(operation, "project" | "expand") {
        return Err("expected project or expand".into());
    }
    let input = if path == "-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| error.to_string())?;
        input
    } else {
        std::fs::read_to_string(path).map_err(|error| error.to_string())?
    };
    let value: Value = serde_json::from_str(&input).map_err(|error| error.to_string())?;
    if operation == "expand" {
        serde_json::to_value(expand(&value)?).map_err(|error| error.to_string())
    } else {
        serde_json::from_value::<ProjectionRequest>(value)
            .map_err(|error| error.to_string())?
            .project()
    }
}
