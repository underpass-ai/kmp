/* KMP ChronoLoom — whole paths, as meaning.
   The view state carries the answer of one kmp_curate path search: chains of
   hops, each declared by a writer or proposed by TypeSafe Jev, plus the
   declarations the audit avoided. This module turns that answer into what
   the scene and the panel draw, and composes the kmp_curate apply call that
   declares a proposed hop. It never calls it: the loom does not write memory.
   Pure: no DOM, no Three.js, no fetch. Exposes KMP_APP.pathModel. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.pathModel = (() => {
  const CONFIDENCES = ["high", "medium", "low", "unknown"];

  /* The key a stored relation is drawn under, so a declared hop and the
     ordinary relation it is can be recognised as one. */
  const edgeKey = (source, rel, target) => `${source} ${rel} ${target}`;

  /* Two decimals are all a person compares; the wire keeps thousandths. */
  function confidenceText(value) {
    return Number.isFinite(value) ? value.toFixed(2) : "–";
  }

  function hopLabel(hop) {
    const rel = hop.rel || "untyped";
    return hop.kind === "declared" ? rel : `${rel} · ${confidenceText(hop.confidence)}`;
  }

  /* One hop as the scene draws it. A declared hop walked against its stored
     direction is drawn the way it was declared, so its arrow tells the
     truth about the relation rather than about the walk. */
  function hopModel(hop, kind, chain, index) {
    const from = hop.from?.ref, to = hop.to?.ref;
    const stored = kind !== "proposed" && hop.reversed;
    const source = stored ? to : from, target = stored ? from : to;
    return {
      key: `${kind} ${edgeKey(source, hop.rel || "", target)}`,
      source,
      target,
      rel: hop.rel || null,
      kind,
      confidence: kind === "avoided" ? Number(hop.support) : Number(hop.confidence),
      item_id: kind === "proposed" ? hop.item_id || null : null,
      from: hop.from || {},
      to: hop.to || {},
      chain,
      index,
    };
  }

  /* The drawn paths, or null when none are drawn. Hops shared by several
     chains are drawn once; every chain still lists them. */
  function overlay(paths) {
    if (!paths || !paths.from) return null;
    const chains = (paths.paths || []).map((chain, number) => ({
      number: number + 1,
      proposed: Number(chain.proposed) || 0,
      confidence: Number(chain.confidence),
      hops: (chain.hops || []).map((hop, index) =>
        hopModel(hop, hop.declared ? "declared" : "proposed", number + 1, index),
      ),
    }));
    const avoided = (paths.avoided || []).map((hop, index) => hopModel(hop, "avoided", 0, index));
    const unique = new Map();
    for (const hop of [...chains.flatMap((chain) => chain.hops), ...avoided])
      if (!unique.has(hop.key)) unique.set(hop.key, hop);
    const refs = new Set([paths.from, ...(paths.to ? [paths.to] : [])]);
    for (const hop of unique.values()) {
      refs.add(hop.source);
      refs.add(hop.target);
    }
    return {
      from: paths.from,
      to: paths.to || null,
      reviewToken: paths.review_token || null,
      summary: paths.summary || "",
      warnings: paths.warnings || [],
      chains,
      avoided,
      hops: [...unique.values()],
      refs,
    };
  }

  /* The hops the scene can draw: both ends placed in the current frame.
     The rest stay listed in the panel, named as outside the frame. */
  function sceneHops(drawn, placed) {
    if (!drawn) return [];
    return drawn.hops.filter((hop) => placed.has(hop.source) && placed.has(hop.target));
  }

  /* Refs a path touches that the primary about owns — the only ones the
     loom can frame by reading its own about. */
  function framingRefs(drawn, about) {
    if (!drawn) return [];
    const refs = new Set();
    for (const hop of drawn.hops) {
      if (hop.from.about === about) refs.add(hop.from.ref);
      if (hop.to.about === about) refs.add(hop.to.ref);
    }
    return [...refs];
  }

  /* The kmp_curate apply call that declares one proposed hop with the
     person's own why and evidence. The about is the hop's source's owner:
     apply writes only items whose `from` belongs to `about`. Returns the
     reason instead when the call could not be honoured as asked. */
  function declareCall(drawn, hop, answer) {
    if (!drawn?.reviewToken) return { error: "This path search froze no review; ask the agent to search again." };
    if (!hop || hop.kind !== "proposed" || !hop.item_id)
      return { error: "Only a proposed step can be declared." };
    const why = String(answer?.why || "").trim();
    const evidence = String(answer?.evidence || "").trim();
    if (!why) return { error: "Write why the link holds — the reason is yours, never Jev's." };
    if (!evidence) return { error: "Say what in the sources shows it." };
    const item = { item_id: hop.item_id, why, evidence };
    const confidence = String(answer?.confidence || "");
    if (CONFIDENCES.includes(confidence)) item.confidence = confidence;
    const call = {
      tool: "kmp_curate",
      arguments: {
        mode: "apply",
        about: hop.from.about,
        review_token: drawn.reviewToken,
        accepted: [item],
      },
    };
    const actor = String(answer?.actor || "").trim();
    if (actor) call.arguments.actor = actor;
    return { call };
  }

  return { CONFIDENCES, edgeKey, confidenceText, hopLabel, overlay, sceneHops, framingRefs, declareCall };
})();
