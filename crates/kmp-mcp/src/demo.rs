//! The worked example a fresh install can look at before writing anything.
//!
//! One fictional checkout service, one incident, one decision that replaced
//! an earlier one, and the measurement that verified it: enough history for
//! every memory move to have something to show. It is a canonical packet,
//! written through the ordinary `kmp_ingest`, so what the demo stores is
//! exactly what a writer could have stored.
use serde_json::Value;

/// The about the demo lives in. It is machine-local, like the guides: a
/// project's committed bundle never carries it.
pub const ABOUT: &str = "example:kmp-demo";

const REQUESTS: &str = include_str!("../fixtures/demo/kmp-demo.requests.json");

/// Questions the demo memory can answer, in the words a person would use.
pub const ASKS: [&str; 4] = [
    "Why are retries to the payment provider capped at two?",
    "What did we believe about retries in August 2026?",
    "What changed about retries in September 2026?",
    "Show me the memory behind the retry cap decision.",
];

/// The packets the demo writes, exactly as `kmp_ingest` receives them.
pub fn requests() -> Result<Vec<Value>, String> {
    let requests: Vec<Value> = serde_json::from_str(REQUESTS)
        .map_err(|error| format!("the demo packet shipped in this binary is invalid: {error}"))?;
    if requests.is_empty() || requests.iter().any(|request| request["about"] != ABOUT) {
        return Err(format!("the demo packet must write only {ABOUT}"));
    }
    Ok(requests)
}

/// How many memories the demo holds, for the line that announces them.
pub fn entry_count() -> Result<usize, String> {
    Ok(requests()?
        .iter()
        .map(|request| request["memory"]["entries"].as_array().map_or(0, Vec::len))
        .sum())
}

/// Write the demo into the store behind `server`. The packet carries one
/// idempotency key, so a store that already holds it appends nothing.
pub async fn seed(server: &crate::KernelMcpServer) -> Result<usize, String> {
    let requests = requests()?;
    crate::guide::ingest_through(server, &requests, "demo").await?;
    entry_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_demo_is_one_packet_about_the_demo_about() {
        let requests = requests().expect("demo packet");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["about"], ABOUT);
        assert_eq!(entry_count().expect("count"), 7);
        for entry in requests[0]["memory"]["entries"]
            .as_array()
            .expect("entries")
        {
            assert!(
                entry["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("example:kmp-demo:")),
                "{entry}"
            );
        }
    }
}
