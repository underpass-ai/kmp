//! Developer control adapter for the native library solver, not an MCP tool or
//! supported query language. Reads a frozen request and an isolated stored graph.
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

fn string(value: &Value) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::from_value(value.clone())?)
}

fn request(value: &Value) -> Result<EvidencePathRequest, Box<dyn Error>> {
    let mut roles = Vec::new();
    for row in value["roles"].as_array().ok_or("roles must be an array")? {
        let mut steps = Vec::new();
        for step in row["steps"].as_array().ok_or("steps must be an array")? {
            steps.push(TraceRelationStep {
                relation: MemoryRelationType::new(string(&step[0])?)?,
                direction: match step[1].as_str() {
                    Some("outgoing") => RelationDirection::Outgoing,
                    Some("incoming") => RelationDirection::Incoming,
                    _ => return Err("invalid direction".into()),
                },
            });
        }
        let mut bindings = Vec::new();
        for binding in row["bindings"]
            .as_array()
            .ok_or("bindings must be an array")?
        {
            let at = u32::try_from(
                binding["at"]
                    .as_u64()
                    .ok_or("binding.at must be unsigned")?,
            )?;
            let name = string(&binding["variable"])?;
            bindings.push(match binding["kind"].as_str() {
                Some("label") => EvidencePathBinding::Label {
                    at,
                    name,
                    key: string(&binding["key"])?,
                },
                Some("ref") => EvidencePathBinding::Reference { at, name },
                _ => return Err("invalid binding kind".into()),
            });
        }
        roles.push(EvidencePathRole {
            name: string(&row["name"])?,
            context: false,
            steps,
            bindings,
        });
    }
    Ok(EvidencePathRequest {
        proof: false,
        about: string(&value["about"])?,
        from: string(&value["from"])?,
        roles,
        constants: serde_json::from_value::<BTreeMap<String, BTreeSet<String>>>(
            value["constants"].clone(),
        )?,
        temporal: TemporalSelection::as_of(
            TemporalCursor::time(string(&value["cut"])?)?,
            TemporalAxis::Observed,
        )?,
        limits: TraceSearchLimits {
            nodes: 4096,
            edges: 32768,
            depth: 1024,
            states: 32768,
        },
        body: Default::default(),
    })
}

fn bindings(value: &EvidencePathBindings) -> Value {
    json!({"domains":value.domains,"missing":value.missing.iter().map(|m|
        json!({"role":m.role,"at":m.at,"ref":m.reference,"key":m.key,"variable":m.name})).collect::<Vec<_>>()})
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: evidence_paths_control STORE_DIRECTORY REQUEST_JSON".into());
    }
    let input: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let request = request(&input)?;
    let store = EmbeddedKernelStore::open(std::path::Path::new(&args[1]))?;
    let result = store.load_evidence_paths(&request).await?;
    let status = match result.status {
        EvidencePathStatus::Compatible => "compatible",
        EvidencePathStatus::Ambiguous => "ambiguous",
        EvidencePathStatus::ReviewRequired => "review_required",
        EvidencePathStatus::MissingObligation => "missing_obligation",
        EvidencePathStatus::IncompatibleObligations => "incompatible_obligations",
        EvidencePathStatus::Partial => "partial",
    };
    let output = json!({"status":status,"stop":result.stop.as_str(),"known_complete":result.known_complete(),
        "missing_roles":result.missing_roles,"viable_candidates":result.viable_candidates(),
        "candidates":result.candidates.iter().map(|c| json!({"role":request.roles[c.role].name,
            "nodes":c.nodes,"edge_indexes":c.edge_indexes,"bindings":bindings(&c.bindings),"clock_unknown":c.clock_unknown})).collect::<Vec<_>>(),
        "groups":result.groups.iter().map(|g| json!({"candidate_indexes":g.candidate_indexes,
            "bindings":bindings(&g.bindings),"clock_unknown":g.clock_unknown})).collect::<Vec<_>>(),
        "relations":result.relations.iter().map(|e| json!({"from":e.source_node_id,"to":e.target_node_id,
            "rel":e.relation_type,"why":e.explanation.rationale(),"evidence":e.explanation.evidence(),
            "observed_at":e.explanation.observed_at().map(|at|
                temporal_instant_rfc3339(at).unwrap_or_else(|| at.to_owned()))})).collect::<Vec<_>>(),
        "work":{"nodes":result.discovered_nodes,"edges":result.scanned_edges,"states":result.work_states,
            "shared_states":result.shared_states,"incompatible_states":result.incompatible_states,
            "adjacency_pages":result.adjacency_pages,"coordinate_pages":result.coordinate_pages},
        "resolved_as_of":result.resolved_as_of,"temporal_selection_resolved":result.temporal_selection_resolved,
        "clock_unknown_edges":result.clock_unknown_edges});
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
