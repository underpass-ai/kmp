//! Source-bound slices are checked before admission and used only when smaller.
use super::{ContextGroup, passage_pool, source_text::SourceText};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(groups: &[ContextGroup]) -> Result<(), String> {
    if groups.iter().all(|group| group.spans.is_empty()) {
        return Ok(());
    }
    let packets = json!(
        groups
            .iter()
            .flat_map(|group| group.packets.clone())
            .collect::<Vec<_>>()
    );
    let sources = SourceText::collect(&packets);
    for group in groups {
        let mut locations = BTreeSet::new();
        for span in &group.spans {
            let fail = || {
                format!(
                    "invalid source span in group {:?} at packet {} {}",
                    group.id, span.packet, span.pointer
                )
            };
            if !locations.insert((span.packet, &span.pointer))
                || !passage_pool::is_prose_pointer(&span.pointer)
            {
                return Err(fail());
            }
            let source = sources
                .iter()
                .find(|source| {
                    source.reference == span.source_ref && source.sha256 == span.source_sha256
                })
                .ok_or_else(|| {
                    format!("{}: source ref/text fingerprint is not returned", fail())
                })?;
            let quote = group
                .packets
                .get(span.packet)
                .and_then(|packet| packet.pointer(&span.pointer))
                .and_then(Value::as_str)
                .ok_or_else(fail)?;
            if span.start_utf8 >= span.end_utf8
                || source.text.get(span.start_utf8..span.end_utf8) != Some(quote)
            {
                return Err(format!(
                    "{}: range must match the whole quote at UTF-8 boundaries",
                    fail()
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn share(packets: &mut Value, groups: &[&ContextGroup]) -> BTreeMap<String, String> {
    let sources = SourceText::collect(packets);
    let mut bindings = Vec::new();
    let mut offset = 0;
    for group in groups {
        for span in &group.spans {
            // A source may have been omitted as a whole group. Keep the quote
            // inline then; never smuggle that source back through its table.
            if let Some(source) = sources.iter().find(|source| {
                source.reference == span.source_ref && source.sha256 == span.source_sha256
            }) {
                bindings.push((offset + span.packet, span, source));
            }
        }
        offset += group.packets.len();
    }
    let mut boundaries = BTreeMap::<String, BTreeSet<usize>>::new();
    for (_, span, source) in &bindings {
        boundaries.entry(source.text.clone()).or_default().extend([
            0,
            source.text.len(),
            span.start_utf8,
            span.end_utf8,
        ]);
    }
    let pieces = boundaries
        .iter()
        .map(|(text, points)| {
            let points = points.iter().copied().collect::<Vec<_>>();
            let pieces = points
                .windows(2)
                .map(|pair| (pair[0], pair[1], text[pair[0]..pair[1]].to_owned()))
                .collect::<Vec<_>>();
            (text, pieces)
        })
        .collect::<BTreeMap<_, _>>();
    let fragments = pieces
        .values()
        .flatten()
        .map(|(_, _, text)| text.clone())
        .collect::<BTreeSet<_>>();
    let names = fragments
        .into_iter()
        .enumerate()
        .map(|(i, text)| (text, format!("p{}", i + 1)))
        .collect::<BTreeMap<_, _>>();
    let marker = |text: &String, start: usize, end: usize| {
        let ids = pieces[text]
            .iter()
            .filter(|(left, right, _)| *left >= start && *right <= end)
            .map(|(_, _, text)| names[text].clone())
            .collect::<Vec<_>>();
        if ids.len() == 1 {
            json!({"passage":ids[0]})
        } else {
            json!({"passage":ids})
        }
    };
    for source in &sources {
        if pieces.contains_key(&source.text) {
            *packets[source.packet]
                .pointer_mut(&source.pointer)
                .expect("collected definition") = marker(&source.text, 0, source.text.len());
        }
    }
    for (packet, span, source) in bindings {
        *packets[packet]
            .pointer_mut(&span.pointer)
            .expect("validated quote") = marker(&source.text, span.start_utf8, span.end_utf8);
    }
    let mut passages = names.into_iter().map(|(text, id)| (id, text)).collect();
    passage_pool::share_into(packets, &mut passages);
    passages
}
