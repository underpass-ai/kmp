//! Lossless composition of existing native packets, independent of a tokenizer.
//!
//! A host declares whole proof groups and their original retrieval calls. KMP
//! shares exact prose without changing refs, provenance, support or clocks.
//! Group admission is ordered and whole: omission never truncates a qualifier.
//! This is representation, not evidence selection or semantic consolidation.

mod citation_pool;
mod context_group;
mod passage_pool;

pub use context_group::ContextGroup;
use serde_json::{Value, json};

/// Compose complete caller-declared groups under a serialized UTF-8 byte limit.
///
/// Every omitted group retains its id and original native reads. The omission
/// manifest is a stable floor; if even that cannot fit, it is returned intact
/// with an explicit warning. The caller owns its tokenizer and prompt framing.
/// Native page/selection omissions remain in the admitted packets unchanged.
pub fn compose(groups: &[ContextGroup], max_bytes: usize) -> Result<Value, String> {
    validate(groups)?;
    let complete = render(groups, &vec![true; groups.len()]);
    if complete.to_string().len() <= max_bytes {
        return Ok(complete);
    }
    let mut admitted = vec![false; groups.len()];
    for index in 0..groups.len() {
        admitted[index] = true;
        if render(groups, &admitted).to_string().len() > max_bytes {
            admitted[index] = false;
        }
    }
    let mut result = render(groups, &admitted);
    let size = result.to_string().len();
    if size > max_bytes {
        result["warnings"] = json!([format!(
            "Context omission manifest requires {size} bytes before this warning; requested {max_bytes}. No group was truncated."
        )]);
    }
    Ok(result)
}

/// Expand admitted groups byte-for-byte at every canonical prose slot.
/// Structural metadata and relationships were never removed or merged.
pub fn expand(projection: &Value) -> Result<Vec<ContextGroup>, String> {
    if projection["contract"] != "kmp.context.passages.v1" {
        return Err("unsupported context projection contract".into());
    }
    let passages = serde_json::from_value(projection["passages"].clone())
        .map_err(|error| format!("invalid passage table: {error}"))?;
    let citations = serde_json::from_value(projection["citations"].clone())
        .map_err(|error| format!("invalid citation table: {error}"))?;
    let mut groups: Vec<ContextGroup> = serde_json::from_value(projection["groups"].clone())
        .map_err(|error| format!("invalid projected groups: {error}"))?;
    for group in &mut groups {
        let mut packets = json!(group.packets);
        passage_pool::restore(&mut packets, &passages)?;
        citation_pool::restore(&mut packets, &citations)?;
        group.packets = serde_json::from_value(packets).map_err(|error| error.to_string())?;
    }
    Ok(groups)
}

/// Opt-in representation of one native packet, using the same lossless pool as
/// composed contexts. No selection or admission changes at this boundary.
/// Returns the original shape if sharing would not reduce its serialized size.
pub fn share_packet(packet: Value) -> Value {
    if !packet.is_object()
        || (packet.get("passages").is_some() || packet.get("citations").is_some())
    {
        return packet;
    }
    if passage_pool::has_references(&mut json!([&packet]))
        || citation_pool::has_references(&mut json!([&packet]))
    {
        return packet;
    }
    let original_bytes = packet.to_string().len();
    let mut packets = json!([packet]);
    let passages = passage_pool::share(&mut packets);
    let citations = citation_pool::share(&mut packets);
    let mut shared = packets[0].take();
    if !passages.is_empty() {
        shared["passages"] = json!(passages);
    }
    if !citations.is_empty() {
        shared["citations"] = json!(citations);
    }
    if shared.to_string().len() < original_bytes {
        shared
    } else {
        // Undo candidate replacements when the table's outer field costs more.
        let mut packets = json!([shared]);
        passage_pool::restore(&mut packets, &passages).expect("new table resolves");
        citation_pool::restore(&mut packets, &citations).expect("new citation table resolves");
        let mut original = packets[0].take();
        if let Some(object) = original.as_object_mut() {
            object.remove("passages");
            object.remove("citations");
        }
        original
    }
}

/// Reconstruct an opt-in packet. The passage table belongs to this one response;
/// do not resolve its names against a table from another page or call.
pub fn expand_packet(mut packet: Value) -> Result<Value, String> {
    if packet.get("passages").is_none() && packet.get("citations").is_none() {
        return Ok(packet);
    }
    let object = packet
        .as_object_mut()
        .ok_or("shared packet must be an object")?;
    let passages = serde_json::from_value(object.remove("passages").unwrap_or_else(|| json!({})))
        .map_err(|error| format!("invalid passage table: {error}"))?;
    let citations = serde_json::from_value(object.remove("citations").unwrap_or_else(|| json!({})))
        .map_err(|error| format!("invalid citation table: {error}"))?;
    let mut packets = json!([packet]);
    passage_pool::restore(&mut packets, &passages)?;
    citation_pool::restore(&mut packets, &citations)?;
    Ok(packets[0].take())
}

fn render(groups: &[ContextGroup], admitted: &[bool]) -> Value {
    let mut all_packets = json!(
        groups
            .iter()
            .zip(admitted)
            .filter(|(_, keep)| **keep)
            .flat_map(|(group, _)| group.packets.clone())
            .collect::<Vec<_>>()
    );
    let passages = passage_pool::share(&mut all_packets);
    let citations = citation_pool::share(&mut all_packets);
    let mut packets = all_packets.as_array().expect("packet array").iter();
    let rendered = groups.iter().zip(admitted).filter(|(_,keep)| **keep).map(|(group,_)| {
        json!({"id":group.id,"packets":packets.by_ref().take(group.packets.len()).cloned().collect::<Vec<_>>(),"reads":group.reads})
    }).collect::<Vec<_>>();
    let omitted = groups
        .iter()
        .zip(admitted)
        .filter(|(_, keep)| !**keep)
        .map(|(group, _)| json!({"id":group.id,"reason":"byte_budget","reads":group.reads}))
        .collect::<Vec<_>>();
    json!({"contract":"kmp.context.passages.v1","groups":rendered,"passages":passages,"citations":citations,"omitted":omitted,"warnings":[]})
}

fn validate(groups: &[ContextGroup]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for group in groups {
        if group.id.is_empty() || !ids.insert(&group.id) {
            return Err("proof group ids must be nonempty and unique".into());
        }
        if passage_pool::has_references(&mut json!(group.packets))
            || citation_pool::has_references(&mut json!(group.packets))
        {
            return Err(
                "expand response-local passage and citation references before composition".into(),
            );
        }
        if group.packets.is_empty()
            || group.packets.iter().any(|packet| {
                !packet.is_object()
                    || (packet.get("passages").is_some() || packet.get("citations").is_some())
            })
        {
            return Err("compose canonical object packets; expand shared packets first".into());
        }
        if group.reads.is_empty()
            || group.reads.iter().any(|read| {
                read["tool"].as_str().is_none_or(|tool| {
                    !matches!(
                        tool,
                        "kmp_ask"
                            | "kmp_wake"
                            | "kmp_inspect"
                            | "kmp_trace"
                            | "kmp_relate"
                            | "kmp_goto"
                            | "kmp_near"
                            | "kmp_rewind"
                            | "kmp_forward"
                    )
                }) || !read["arguments"].is_object()
                    || !read["arguments"]["about"].is_string()
                    || read["arguments"].get("continuation").is_some()
            })
        {
            return Err("each proof group needs explicit original memory reads with about, not expiring continuations".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
