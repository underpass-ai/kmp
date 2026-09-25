use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{
    next_step_request, pair_request, suspect_request,
};
use crate::curate::application::on_the_way::facts_on_the_way;
use crate::curate::application::path_search::PathSearch;
use crate::curate::domain::avoided_hop::AvoidedHop;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_thresholds::{DOUBT_BELOW, NONE};
use crate::curate::domain::found_path::FoundPath;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::curate::domain::path_hop::PathHop;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

const MAX_PATHS: usize = 3;
/// A proposed step needs at least this probability, and a fact contributes
/// at most this many in each direction: the judge's second choice is often
/// the other thing a fact led to.
const STEP_AT: f64 = 0.3;
const STEPS_PER_FACT: usize = 2;
/// Bound on the partial walks a goal-less search expands.
const MAX_WALKS: usize = 20_000;
/// Rounds of audit and search: each audits the declared hops the paths now
/// walk and searches again without the ones whose reason does not hold.
const AUDIT_ROUNDS: usize = 3;

/// Whole paths across the selection: the relations writers declared, and,
/// with the judge, the steps it proposes between facts that lie on the way.
/// Proposed steps are suggestions for the agent to declare, never proof.
pub(crate) struct FindPaths<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
}

impl FindPaths<'_> {
    pub(crate) async fn run(
        &self,
        material: &CurateMaterial,
        from: &str,
        to: Option<&str>,
        max_hops: usize,
    ) -> PathSearch {
        let mut search = PathSearch {
            paths: Vec::new(),
            considered: material.facts.len(),
            kept: 0,
            avoided: Vec::new(),
            jev: None,
            warnings: Vec::new(),
        };
        for end in std::iter::once(from).chain(to) {
            if material.fact(end).is_none() {
                search
                    .warnings
                    .push(format!("`{end}` is not a current fact of the selection"));
                return search;
            }
        }
        let mut edges = material
            .declared
            .iter()
            .map(|link| PathHop {
                from: link.from.clone(),
                to: link.to.clone(),
                rel: Some(link.rel.clone()),
                declared: true,
                confidence: 1.0,
                reversed: false,
            })
            .collect::<Vec<_>>();
        let mut banned = BTreeSet::new();
        let Some(model) = self.judgement else {
            search.warnings.push(
                "Jev is not configured for this store; paths follow declared relations only".into(),
            );
            search.paths = walk(&edges, from, to, max_hops, &banned);
            return search;
        };
        let mut usage = JevUsage {
            model: model.model().to_string(),
            requests: 0,
            input_tokens: 0,
        };
        if let Err(error) = propose(
            model,
            material,
            from,
            to,
            &mut edges,
            &mut search,
            &mut usage,
        )
        .await
        {
            search
                .warnings
                .push(format!("Jev unavailable; declared relations only: {error}"));
        }
        // Declared hops come first in `edges`, in the order of
        // `material.declared`.
        let mut audited = BTreeSet::new();
        search.paths = walk(&edges, from, to, max_hops, &banned);
        for _ in 0..AUDIT_ROUNDS {
            let fresh = walked_declared(&edges, &search.paths)
                .into_iter()
                .filter(|index| audited.insert(*index))
                .collect::<Vec<_>>();
            if fresh.is_empty() {
                break;
            }
            let doubted = match audit(model, material, &fresh, &mut usage).await {
                Ok(doubted) => doubted,
                Err(error) => {
                    search.warnings.push(format!(
                        "Jev unavailable; declared relations were walked unaudited: {error}"
                    ));
                    break;
                }
            };
            if doubted.is_empty() {
                break;
            }
            for (index, support) in doubted {
                banned.insert(index);
                search.avoided.push(AvoidedHop {
                    hop: edges[index].clone(),
                    support,
                });
            }
            search.paths = walk(&edges, from, to, max_hops, &banned);
        }
        search.jev = Some(usage);
        search
    }
}

fn walk(
    edges: &[PathHop],
    from: &str,
    to: Option<&str>,
    max_hops: usize,
    banned: &BTreeSet<usize>,
) -> Vec<FoundPath> {
    match to {
        Some(to) => shortest_paths(edges, from, to, max_hops, banned),
        None => longest_chains(edges, from, max_hops, banned),
    }
}

/// Indices of the declared edges the paths walk, either way round.
fn walked_declared(edges: &[PathHop], paths: &[FoundPath]) -> BTreeSet<usize> {
    paths
        .iter()
        .flat_map(|path| &path.hops)
        .filter(|hop| hop.declared)
        .filter_map(|hop| {
            edges.iter().position(|edge| {
                edge.declared
                    && edge.rel == hop.rel
                    && ((edge.from == hop.from && edge.to == hop.to)
                        || (edge.from == hop.to && edge.to == hop.from))
            })
        })
        .collect()
}

/// Ask whether the why and evidence of each walked declaration hold; the
/// ones below the doubt line come back with their support.
async fn audit(
    model: &dyn JudgementModel,
    material: &CurateMaterial,
    indices: &[usize],
    usage: &mut JevUsage,
) -> Result<Vec<(usize, f64)>, String> {
    let walked = CurateMaterial {
        declared: indices
            .iter()
            .filter_map(|index| material.declared.get(*index).cloned())
            .collect(),
        ..material.clone()
    };
    let response = model.evaluate(&suspect_request(&walked)).await?;
    usage.requests += response.requests;
    usage.input_tokens += response.input_tokens;
    Ok(indices
        .iter()
        .enumerate()
        .filter_map(|(n, index)| match response.answers.get(&format!("s{n}")) {
            Some(JudgementAnswer::Noul { yes }) if *yes < DOUBT_BELOW => Some((*index, *yes)),
            _ => None,
        })
        .collect())
}

async fn propose(
    model: &dyn JudgementModel,
    material: &CurateMaterial,
    from: &str,
    to: Option<&str>,
    edges: &mut Vec<PathHop>,
    search: &mut PathSearch,
    usage: &mut JevUsage,
) -> Result<(), String> {
    let kept = facts_on_the_way(model, material, from, to, usage).await?;
    search.kept = kept.len();
    let walk = std::iter::once(from.to_string())
        .chain(to.map(str::to_string))
        .chain(kept)
        .collect::<Vec<_>>();
    if walk.len() < 2 {
        return Ok(());
    }
    let (request, keys) = next_step_request(material, &walk);
    let response = model.evaluate(&request).await?;
    usage.requests += response.requests;
    usage.input_tokens += response.input_tokens;
    let joined = |left: &str, right: &str| {
        edges.iter().any(|hop| {
            (hop.from == left && hop.to == right) || (hop.from == right && hop.to == left)
        })
    };
    let mut proposed: Vec<PathHop> = Vec::new();
    for (key, reference) in &keys {
        for (question, forward) in [("n", true), ("c", false)] {
            let Some(JudgementAnswer::Choice { probabilities, .. }) =
                response.answers.get(&format!("{question}{}", &key[1..]))
            else {
                continue;
            };
            let mut options = probabilities
                .iter()
                .filter(|(option, p)| option.as_str() != NONE && **p >= STEP_AT)
                .collect::<Vec<_>>();
            options
                .sort_by(|left, right| right.1.total_cmp(left.1).then_with(|| left.0.cmp(right.0)));
            for (option, p) in options.into_iter().take(STEPS_PER_FACT) {
                let Some(other) = keys.get(option.as_str()) else {
                    continue;
                };
                let (from, to) = if forward {
                    (reference.clone(), other.clone())
                } else {
                    (other.clone(), reference.clone())
                };
                let seen = joined(&from, &to)
                    || proposed.iter().any(|hop| {
                        (hop.from == from && hop.to == to) || (hop.from == to && hop.to == from)
                    });
                if !seen {
                    proposed.push(PathHop {
                        from,
                        to,
                        rel: None,
                        declared: false,
                        confidence: *p,
                        reversed: false,
                    });
                }
            }
        }
    }
    if !proposed.is_empty() {
        let pairs = proposed
            .iter()
            .map(|hop| CandidatePair {
                from: hop.from.clone(),
                to: hop.to.clone(),
                origin: PairOrigin::Jev,
                crosses_abouts: material.fact(&hop.from).map(|f| &f.about)
                    != material.fact(&hop.to).map(|f| &f.about),
            })
            .collect::<Vec<_>>();
        let typed = model.evaluate(&pair_request(material, &pairs)).await?;
        usage.requests += typed.requests;
        usage.input_tokens += typed.input_tokens;
        for (n, hop) in proposed.iter_mut().enumerate() {
            if let Some(JudgementAnswer::Choice { choice, .. }) =
                typed.answers.get(&format!("t{n}"))
                && choice != NONE
            {
                hop.rel = Some(choice.clone());
            }
        }
        // A step the judge types as no relation is not a step.
        proposed.retain(|hop| hop.rel.is_some());
    }
    edges.extend(proposed);
    Ok(())
}

/// Neighbours of a fact over every edge, walked either way.
fn neighbours(edges: &[PathHop]) -> BTreeMap<&str, Vec<(usize, &str, bool)>> {
    let mut adjacency = BTreeMap::<&str, Vec<(usize, &str, bool)>>::new();
    for (index, hop) in edges.iter().enumerate() {
        adjacency
            .entry(hop.from.as_str())
            .or_default()
            .push((index, hop.to.as_str(), false));
        adjacency
            .entry(hop.to.as_str())
            .or_default()
            .push((index, hop.from.as_str(), true));
    }
    adjacency
}

fn hop_along(edges: &[PathHop], index: usize, reversed: bool) -> PathHop {
    let stored = &edges[index];
    let mut hop = stored.clone();
    if reversed {
        hop.from = stored.to.clone();
        hop.to = stored.from.clone();
        hop.reversed = true;
    }
    hop
}

/// Fewest hops first, then fewest proposed hops: a declared route beats a
/// judged one of the same length.
fn shortest(
    edges: &[PathHop],
    from: &str,
    to: &str,
    max_hops: usize,
    banned: &BTreeSet<usize>,
) -> Option<FoundPath> {
    let adjacency = neighbours(edges);
    let mut best = BTreeMap::<&str, (usize, usize)>::new();
    let mut back = BTreeMap::<&str, (&str, usize, bool)>::new();
    let mut queue = BinaryHeap::new();
    queue.push(std::cmp::Reverse((0usize, 0usize, from)));
    best.insert(from, (0, 0));
    while let Some(std::cmp::Reverse((hops, proposed, node))) = queue.pop() {
        if node == to {
            let mut path = Vec::new();
            let mut at = to;
            while let Some((previous, index, reversed)) = back.get(at) {
                path.push(hop_along(edges, *index, *reversed));
                at = previous;
            }
            path.reverse();
            return Some(FoundPath { hops: path });
        }
        if hops >= max_hops || best.get(node).is_some_and(|seen| *seen < (hops, proposed)) {
            continue;
        }
        for (index, next, reversed) in adjacency.get(node).into_iter().flatten() {
            if banned.contains(index) {
                continue;
            }
            let cost = (hops + 1, proposed + usize::from(!edges[*index].declared));
            if best.get(next).is_none_or(|seen| cost < *seen) {
                best.insert(next, cost);
                back.insert(next, (node, *index, *reversed));
                queue.push(std::cmp::Reverse((cost.0, cost.1, *next)));
            }
        }
    }
    None
}

fn shortest_paths(
    edges: &[PathHop],
    from: &str,
    to: &str,
    max_hops: usize,
    avoided: &BTreeSet<usize>,
) -> Vec<FoundPath> {
    let Some(first) = shortest(edges, from, to, max_hops, avoided) else {
        return Vec::new();
    };
    let mut paths = vec![first.clone()];
    // Alternatives: the same search with one proposed step of the first
    // path taken away at a time.
    for hop in first.hops.iter().filter(|hop| !hop.declared) {
        let banned = edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                (edge.from == hop.from && edge.to == hop.to)
                    || (edge.from == hop.to && edge.to == hop.from)
            })
            .map(|(index, _)| index)
            .chain(avoided.iter().copied())
            .collect::<BTreeSet<_>>();
        if let Some(path) = shortest(edges, from, to, max_hops, &banned)
            && !paths.contains(&path)
        {
            paths.push(path);
        }
        if paths.len() >= MAX_PATHS {
            break;
        }
    }
    paths
}

/// With no goal: the longest simple walks from the start, strongest first.
fn longest_chains(
    edges: &[PathHop],
    from: &str,
    max_hops: usize,
    avoided: &BTreeSet<usize>,
) -> Vec<FoundPath> {
    let adjacency = neighbours(edges);
    let mut finished = Vec::new();
    let mut stack = vec![(vec![from.to_string()], Vec::<PathHop>::new())];
    let mut expanded = 0;
    while let Some((visited, hops)) = stack.pop() {
        expanded += 1;
        if expanded > MAX_WALKS {
            break;
        }
        let at = visited.last().cloned().unwrap_or_default();
        let mut extended = false;
        if hops.len() < max_hops {
            for (index, next, reversed) in adjacency.get(at.as_str()).into_iter().flatten() {
                if avoided.contains(index) || visited.iter().any(|seen| seen == next) {
                    continue;
                }
                let mut visited = visited.clone();
                visited.push((*next).to_string());
                let mut hops = hops.clone();
                hops.push(hop_along(edges, *index, *reversed));
                stack.push((visited, hops));
                extended = true;
            }
        }
        if !extended && !hops.is_empty() {
            finished.push(FoundPath { hops });
        }
    }
    finished.sort_by(|left, right| {
        right
            .hops
            .len()
            .cmp(&left.hops.len())
            .then_with(|| right.confidence().total_cmp(&left.confidence()))
    });
    finished.truncate(MAX_PATHS);
    finished
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(from: &str, to: &str, declared: bool) -> PathHop {
        PathHop {
            from: from.into(),
            to: to.into(),
            rel: Some("supports".into()),
            declared,
            confidence: if declared { 1.0 } else { 0.8 },
            reversed: false,
        }
    }

    #[test]
    fn the_shortest_route_prefers_declared_hops_and_offers_alternatives() {
        let edges = vec![
            hop("a", "b", true),
            hop("b", "d", false),
            hop("a", "c", true),
            hop("c", "d", true),
        ];
        let paths = shortest_paths(&edges, "a", "d", 6, &BTreeSet::new());
        assert_eq!(paths[0].proposed(), 0, "a-c-d is declared all the way");
        assert_eq!(paths.len(), 1, "no proposed hop to take away");
        let edges = vec![
            hop("a", "b", true),
            hop("b", "d", false),
            hop("d", "c", true),
        ];
        let paths = shortest_paths(&edges, "a", "c", 6, &BTreeSet::new());
        assert_eq!(paths[0].hops.len(), 3);
        assert_eq!(paths[0].proposed(), 1);
    }

    #[test]
    fn a_hop_walked_against_its_direction_says_so() {
        let edges = vec![hop("b", "a", true)];
        let paths = shortest_paths(&edges, "a", "b", 6, &BTreeSet::new());
        assert!(paths[0].hops[0].reversed);
        assert_eq!(
            (paths[0].hops[0].from.as_str(), paths[0].hops[0].to.as_str()),
            ("a", "b")
        );
    }

    fn material() -> CurateMaterial {
        use crate::curate::domain::{curate_fact::CurateFact, declared_link::DeclaredLink};
        let fact = |reference: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
            text: format!("text {reference}"),
            occurred: None,
            labels: Vec::new(),
        };
        let link = |from: &str, to: &str| DeclaredLink {
            from: from.into(),
            to: to.into(),
            rel: "supports".into(),
            why: "w".into(),
            evidence: "e".into(),
        };
        CurateMaterial {
            facts: vec![fact("a"), fact("b"), fact("c")],
            declared: vec![link("a", "b"), link("b", "c")],
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_walked_declaration_whose_reason_does_not_hold_is_avoided() {
        use super::super::scripted_judgement::Scripted;
        use std::sync::Mutex;
        let doubting = Scripted {
            noul: 0.1,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let search = FindPaths {
            judgement: Some(&doubting),
        }
        .run(&material(), "a", Some("c"), 6)
        .await;
        assert!(search.paths.is_empty(), "both declarations were doubted");
        assert_eq!(search.avoided.len(), 2);
        assert!(search.avoided.iter().all(|avoided| avoided.support < 0.3));

        let trusting = Scripted {
            noul: 0.9,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let search = FindPaths {
            judgement: Some(&trusting),
        }
        .run(&material(), "a", Some("c"), 6)
        .await;
        assert_eq!(search.paths[0].hops.len(), 2);
        assert!(search.avoided.is_empty());

        let plain = FindPaths { judgement: None }
            .run(&material(), "a", None, 6)
            .await;
        assert_eq!(
            plain.paths[0].hops.len(),
            2,
            "without Jev nothing is audited"
        );
    }

    #[test]
    fn without_a_goal_the_longest_chain_comes_first() {
        let edges = vec![
            hop("a", "b", true),
            hop("b", "c", false),
            hop("a", "x", true),
        ];
        let chains = longest_chains(&edges, "a", 6, &BTreeSet::new());
        assert_eq!(chains[0].hops.len(), 2);
        assert!(shortest_paths(&edges, "a", "zzz", 6, &BTreeSet::new()).is_empty());
    }
}
