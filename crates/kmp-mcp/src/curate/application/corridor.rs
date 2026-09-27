use std::collections::{BTreeMap, HashMap, VecDeque};

use kmp_proto_mapping::v1beta1::PartnerShortlist;

use crate::curate::application::curate_material::CurateMaterial;

/// Facts a search with a goal may walk through, both ends included.
pub(crate) const CORRIDOR_FACTS: usize = 24;
/// How far over kernel pairs and declared relations a corridor fact may lie
/// from either end.
const CORRIDOR_RADIUS: usize = 4;
/// Options each fact's next-step choices offer besides `none`.
pub(crate) const STEP_OPTIONS: usize = 8;

/// The facts between `from` and `to` a goal search asks the judge about
/// (DESIGN L7), in place of the whole selection: the balls of radius
/// [`CORRIDOR_RADIUS`] around each end over kernel pairs and declared
/// relations, nearest to both ends first (d_f + d_b, an end the graph does
/// not reach counting one past the radius), then by how much of the two
/// ends' rarer words they share (BM25 over the selection), then in the
/// selection's order. At most [`CORRIDOR_FACTS`] facts with both ends. When
/// the graph links nothing near either end, the facts sharing the most of
/// the ends' rarer words stand in for the balls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Corridor {
    /// The facts between the ends, nearest first; the ends are not in it.
    pub refs: Vec<String>,
}

impl Corridor {
    pub(crate) fn between(material: &CurateMaterial, from: &str, to: &str) -> Self {
        let adjacency = adjacency(material);
        let forward = distances(&adjacency, from);
        let backward = distances(&adjacency, to);
        let lexical = lexical_rank(material, from, to);
        let beyond = CORRIDOR_RADIUS + 1;
        let linked = forward.len() + backward.len() > 2;
        let mut ranked = material
            .facts
            .iter()
            .enumerate()
            .filter(|(_, fact)| fact.reference != from && fact.reference != to)
            .map(|(order, fact)| {
                let reference = fact.reference.as_str();
                let near = forward.get(reference).copied().unwrap_or(beyond)
                    + backward.get(reference).copied().unwrap_or(beyond);
                let words = lexical.get(reference).copied().unwrap_or(usize::MAX);
                ((near, words, order), reference)
            })
            .filter(|((near, _, _), _)| !linked || *near < 2 * beyond)
            .collect::<Vec<_>>();
        ranked.sort_unstable();
        Self {
            refs: ranked
                .into_iter()
                .take(CORRIDOR_FACTS - 2)
                .map(|(_, reference)| reference.to_string())
                .collect(),
        }
    }

    /// For each fact of `walk`, the at most [`STEP_OPTIONS`] other walk facts
    /// its next-step choices offer: the ones a kernel pair or a declared
    /// relation links it to, then the ones sharing its rarer words, in walk
    /// order on a tie.
    pub(crate) fn step_options(
        material: &CurateMaterial,
        walk: &[String],
    ) -> BTreeMap<String, Vec<String>> {
        let adjacency = adjacency(material);
        let texts = walk
            .iter()
            .map(|reference| {
                let text = material
                    .fact(reference)
                    .map(|fact| fact.text.as_str())
                    .unwrap_or_default();
                (reference.as_str(), text)
            })
            .collect::<Vec<_>>();
        let shortlist = PartnerShortlist::over(texts.iter().copied());
        texts
            .iter()
            .map(|(reference, text)| {
                let linked = adjacency.get(reference).cloned().unwrap_or_default();
                let mut options = walk
                    .iter()
                    .filter(|other| other != reference && linked.contains(&other.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                for other in shortlist.partners(reference, text, walk.len()).refs() {
                    if options.len() >= STEP_OPTIONS {
                        break;
                    }
                    if other != reference && !options.contains(other) {
                        options.push(other.clone());
                    }
                }
                for other in walk {
                    if options.len() >= STEP_OPTIONS {
                        break;
                    }
                    if other != reference && !options.contains(other) {
                        options.push(other.clone());
                    }
                }
                options.truncate(STEP_OPTIONS);
                ((*reference).to_string(), options)
            })
            .collect()
    }
}

/// Whether a step between `left` and `right` can lie on a walk of at most
/// `max_hops` from `from` to `to` over `edges` (either way round): only
/// such a step can reach a returned path, so only such a step is worth
/// typing.
pub(crate) fn on_some_walk(
    edges: &[(&str, &str)],
    from: &str,
    to: &str,
    max_hops: usize,
) -> impl Fn(&str, &str) -> bool {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    for (left, right) in edges {
        adjacency
            .entry((*left).to_string())
            .or_default()
            .push((*right).to_string());
        adjacency
            .entry((*right).to_string())
            .or_default()
            .push((*left).to_string());
    }
    let reach = |start: &str| {
        let mut distance = HashMap::from([(start.to_string(), 0usize)]);
        let mut queue = VecDeque::from([start.to_string()]);
        while let Some(node) = queue.pop_front() {
            let here = distance[&node];
            for next in adjacency.get(&node).into_iter().flatten() {
                if !distance.contains_key(next) {
                    distance.insert(next.clone(), here + 1);
                    queue.push_back(next.clone());
                }
            }
        }
        distance
    };
    let (forward, backward) = (reach(from), reach(to));
    move |left: &str, right: &str| {
        let through = |a: &str, b: &str| {
            forward
                .get(a)
                .zip(backward.get(b))
                .is_some_and(|(f, b)| f + 1 + b <= max_hops)
        };
        through(left, right) || through(right, left)
    }
}

/// Kernel pairs and declared relations, walked either way.
fn adjacency(material: &CurateMaterial) -> HashMap<&str, Vec<&str>> {
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    let edges = material
        .pairs
        .iter()
        .map(|pair| (pair.from.as_str(), pair.to.as_str()))
        .chain(
            material
                .declared
                .iter()
                .map(|link| (link.from.as_str(), link.to.as_str())),
        );
    for (left, right) in edges {
        adjacency.entry(left).or_default().push(right);
        adjacency.entry(right).or_default().push(left);
    }
    adjacency
}

fn distances<'a>(
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    start: &'a str,
) -> HashMap<&'a str, usize> {
    let mut distance = HashMap::from([(start, 0)]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        let here = distance[node];
        if here == CORRIDOR_RADIUS {
            continue;
        }
        for next in adjacency.get(node).into_iter().flatten() {
            if !distance.contains_key(next) {
                distance.insert(next, here + 1);
                queue.push_back(next);
            }
        }
    }
    distance
}

/// Each fact's place among the ones sharing the ends' words, best first.
fn lexical_rank(material: &CurateMaterial, from: &str, to: &str) -> HashMap<String, usize> {
    let text = |reference: &str| {
        material
            .fact(reference)
            .map(|fact| fact.text.clone())
            .unwrap_or_default()
    };
    let ends = format!("{} {}", text(from), text(to));
    PartnerShortlist::over(
        material
            .facts
            .iter()
            .map(|fact| (fact.reference.as_str(), fact.text.as_str())),
    )
    .partners(from, &ends, material.facts.len())
    .refs()
    .enumerate()
    .map(|(rank, reference)| (reference.clone(), rank))
    .collect()
}

#[cfg(test)]
#[path = "corridor_tests.rs"]
mod tests;
