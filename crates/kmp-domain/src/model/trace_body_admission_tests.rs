use super::*;

fn descriptor(node_id: &str, record_bytes: u64) -> NodeBodyDescriptor {
    NodeBodyDescriptor {
        node_id: node_id.into(),
        revision: 1,
        content_hash: format!("hash:{node_id}"),
        record_bytes,
        // Escaping makes the record longer than the text it carries; the
        // ceiling spends record bytes and the loaded total counts these.
        body_bytes: record_bytes.saturating_sub(2),
        record_digest: format!("sha256:{node_id}"),
    }
}

fn table(sizes: &[(&str, u64)]) -> (Vec<String>, BTreeMap<String, NodeBodyDescriptor>) {
    let manifest: Vec<String> = sizes.iter().map(|(id, _)| (*id).to_string()).collect();
    let descriptors = sizes
        .iter()
        .map(|(id, bytes)| ((*id).to_string(), descriptor(id, *bytes)))
        .collect();
    (manifest, descriptors)
}

#[test]
fn without_options_every_present_body_loads_and_nothing_is_measured_against_a_ceiling() {
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4), ("c", 9)]);

    let admitted = admit(
        &manifest,
        &descriptors,
        &TraceBodyOptions::default(),
        &BTreeSet::new(),
    );

    assert_eq!(admitted.load, vec!["a", "b", "c"]);
    assert_eq!(admitted.next_deferred, None);
    assert_eq!(admitted.admitted_record_bytes, 17);
    assert!(admitted.omitted().is_empty());
}

#[test]
fn the_two_next_step_minimums_have_different_scopes() {
    // The worked case from the contract: sizes 4, 4, 9 under a ceiling of 8.
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4), ("c", 9)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(8),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert_eq!(admitted.load, vec!["a", "b"]);
    assert_eq!(admitted.admitted_record_bytes, 8);
    assert_eq!(admitted.state("c"), TraceBodyState::DeferredBudget);
    assert_eq!(
        admitted.next_rerun_record_bytes(),
        Some(17),
        "rerunning the whole query pays for the admitted prefix again"
    );
    assert_eq!(
        admitted.next_named_record_bytes(),
        Some(9),
        "a named batch pays for nothing it did not name"
    );
}

#[test]
fn a_record_over_the_ceiling_defers_everything_after_it_rather_than_skipping_ahead() {
    // The large record comes first. Admitting the small ones behind it would
    // make delivery depend on size instead of order, and would let the large
    // one vanish from a response that looks whole.
    let (manifest, descriptors) = table(&[("a", 64), ("b", 2), ("c", 2)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(8),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert!(admitted.load.is_empty(), "no body is read at all");
    assert_eq!(admitted.admitted_record_bytes, 0);
    for id in ["a", "b", "c"] {
        assert_eq!(admitted.state(id), TraceBodyState::DeferredBudget);
    }
    assert_eq!(
        admitted.next_named_record_bytes(),
        Some(64),
        "the exact requirement is named; nobody is told to raise the budget to the total"
    );
}

#[test]
fn an_oversized_record_never_becomes_absent_evidence() {
    let (manifest, descriptors) = table(&[("huge", 64 * 1024 * 1024)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(8 * 1024 * 1024),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert_eq!(admitted.state("huge"), TraceBodyState::DeferredBudget);
    assert_ne!(
        admitted.state("huge"),
        TraceBodyState::Missing,
        "a body too large for this ceiling is withheld, never reported absent"
    );
    assert_eq!(admitted.next_named_record_bytes(), Some(64 * 1024 * 1024));
    assert!(admitted.load.is_empty());
}

#[test]
fn a_missing_record_costs_nothing_and_stays_missing() {
    let (manifest, mut descriptors) = table(&[("a", 4), ("gone", 4), ("b", 4)]);
    descriptors.remove("gone");
    let options = TraceBodyOptions {
        max_record_bytes: Some(8),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert_eq!(admitted.state("gone"), TraceBodyState::Missing);
    assert_eq!(admitted.load, vec!["a", "b"]);
    assert_eq!(admitted.admitted_record_bytes, 8);
    assert_eq!(
        admitted.selected_body_bytes, 4,
        "an absent body contributes nothing to the selected total"
    );
}

#[test]
fn a_named_expansion_loads_only_what_it_named() {
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4), ("c", 4)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(64),
        refs: Some(BTreeSet::from(["b".to_string()])),
        expect_selection: Some("manifest".into()),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert_eq!(admitted.load, vec!["b"]);
    assert_eq!(admitted.state("a"), TraceBodyState::NotRequested);
    assert_eq!(admitted.state("c"), TraceBodyState::NotRequested);
    assert_eq!(admitted.admitted_record_bytes, 4);
}

#[test]
fn an_empty_named_expansion_is_a_descriptor_only_view() {
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4)]);
    let options = TraceBodyOptions {
        refs: Some(BTreeSet::new()),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());

    assert!(admitted.load.is_empty());
    assert_eq!(admitted.selected_body_bytes, 4, "two records of 2 text bytes");
    assert_eq!(admitted.omitted().len(), 2);
}

#[test]
fn a_compact_read_loads_no_body_at_all_whether_or_not_a_card_is_usable() {
    // `a` and `b` have usable cards; `c` does not. Loading `c` because its
    // card is missing would be the implicit fallback the contract forbids,
    // and it is exactly the record a reader would least want to pay for.
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4), ("c", 8)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(64),
        compact: Some("es".into()),
        ..TraceBodyOptions::default()
    };

    let admitted = admit(
        &manifest,
        &descriptors,
        &options,
        &BTreeSet::from(["a".to_string(), "b".to_string()]),
    );

    assert!(admitted.load.is_empty(), "no canonical record is read");
    assert_eq!(admitted.admitted_record_bytes, 0, "the ceiling is unspent");
    assert_eq!(admitted.state("a"), TraceBodyState::Compact);
    assert_eq!(admitted.state("b"), TraceBodyState::Compact);
    assert_eq!(
        admitted.state("c"),
        TraceBodyState::NotRequested,
        "an uncarded body is named as undelivered, never quietly loaded"
    );
    assert_eq!(
        admitted.omitted().len(),
        3,
        "all three are withheld canonical bodies"
    );
}

#[test]
fn a_named_expansion_is_never_answered_with_a_card() {
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4)]);
    let options = TraceBodyOptions {
        max_record_bytes: Some(64),
        refs: Some(BTreeSet::from(["a".to_string()])),
        expect_selection: Some("manifest".into()),
        compact: Some("es".into()),
    };

    let admitted = admit(
        &manifest,
        &descriptors,
        &options,
        &BTreeSet::from(["a".to_string(), "b".to_string()]),
    );

    assert_eq!(
        admitted.load,
        vec!["a"],
        "the named ref loads its canonical body even though a valid card exists"
    );
    assert_eq!(admitted.state("a"), TraceBodyState::Loaded);
    assert_eq!(
        admitted.state("b"),
        TraceBodyState::Compact,
        "and the refs nobody named keep their cards"
    );
}

#[test]
fn successive_named_batches_recover_every_body_exactly_once_under_a_fixed_ceiling() {
    let (manifest, descriptors) = table(&[("a", 4), ("b", 4), ("c", 9), ("d", 3)]);
    let ceiling = 9;
    let mut recovered: Vec<String> = Vec::new();
    let mut remaining: BTreeSet<String> = manifest.iter().cloned().collect();

    // Each round asks for the refs still missing and keeps the same ceiling.
    for _ in 0..8 {
        if remaining.is_empty() {
            break;
        }
        let options = TraceBodyOptions {
            max_record_bytes: Some(ceiling),
            refs: Some(remaining.clone()),
            expect_selection: Some("manifest".into()),
            ..TraceBodyOptions::default()
        };
        let admitted = admit(&manifest, &descriptors, &options, &BTreeSet::new());
        assert!(
            !admitted.load.is_empty(),
            "a round that admits nothing would be an identical action loop"
        );
        assert!(admitted.admitted_record_bytes <= ceiling);
        for id in &admitted.load {
            remaining.remove(id);
            recovered.push(id.clone());
        }
    }

    assert!(remaining.is_empty(), "every body was recovered");
    let mut once = recovered.clone();
    once.sort();
    once.dedup();
    assert_eq!(
        once.len(),
        recovered.len(),
        "no body was delivered twice across the batches"
    );
    assert_eq!(once, vec!["a", "b", "c", "d"]);
}
