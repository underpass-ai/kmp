//! Compose captured native packets without reading or writing a memory store.
//! Input: {"groups":[ContextGroup,...],"max_bytes":N}; output: projection JSON.
//! --expand accepts that output and reconstructs the admitted original groups.
use kmp_proto_mapping::context_projection::{ContextGroup, compose, expand};
use serde_json::{Value, json};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let output = if std::env::args().nth(1).as_deref() == Some("--expand") {
        json!(expand(&input)?)
    } else {
        let groups: Vec<ContextGroup> = serde_json::from_value(input["groups"].clone())?;
        let budget = input["max_bytes"]
            .as_u64()
            .map(usize::try_from)
            .transpose()?
            .unwrap_or(usize::MAX);
        compose(&groups, budget)?
    };
    println!("{output}");
    Ok(())
}
