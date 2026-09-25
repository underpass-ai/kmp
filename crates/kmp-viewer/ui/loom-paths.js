/* KMP ChronoLoom — the found-paths panel and the declare form.
   DOM adapter over KMP_APP.pathModel. It lists every chain of the drawn
   paths, marks proposed hops, and lets a person declare one in their own
   words. Declaring composes the kmp_curate apply call and copies it for the
   agent to run: the loom observes memory and never writes it, so the write
   keeps the writer's review, Jev's re-read of the why and evidence, and the
   agent's provenance. Exposes KMP_APP.paths. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.paths = (() => {
  const { model, view } = KMP_APP.state;
  const { $, el } = KMP_APP.dom;
  let declaring = null;

  const short = (text, size = 72) => (text && text.length > size ? text.slice(0, size - 1) + "…" : text || "");

  /* A fact end the person can reach: selecting it inspects the record. */
  function endButton(end) {
    const button = el("button", "path-end", short(end.excerpt || end.ref));
    button.type = "button";
    button.title = `${end.about} · ${end.ref}`;
    button.addEventListener("click", () => KMP_APP.selection.selectEntry(end.ref));
    return button;
  }

  function hopItem(hop, placed) {
    const item = el("li", `path-hop path-hop-${hop.kind}`);
    const head = el("div", "path-hop-head");
    const stroke = el("span", `path-stroke path-stroke-${hop.kind}`);
    stroke.setAttribute("aria-hidden", "true");
    const badge =
      hop.kind === "declared"
        ? "declared"
        : hop.kind === "proposed"
          ? `proposed by Jev · ${KMP_APP.pathModel.confidenceText(hop.confidence)}`
          : `avoided · support ${KMP_APP.pathModel.confidenceText(hop.confidence)}`;
    head.append(stroke, el("span", "rel-type", hop.rel || "untyped"), el("span", "pill pill-muted", badge));
    if (!placed.has(hop.source) || !placed.has(hop.target))
      head.append(el("span", "muted path-offframe", "outside the frame"));
    item.append(head);
    const ends = el("div", "path-hop-ends");
    ends.append(endButton(hop.from), el("span", "path-arrow", "→"), endButton(hop.to));
    item.append(ends);
    if (hop.kind === "proposed" && hop.item_id) {
      const declare = el("button", "btn small-btn path-declare", "Declare…");
      declare.type = "button";
      declare.title = `Declare ${hop.item_id} with your own why and evidence`;
      declare.addEventListener("click", () => openDeclare(hop));
      item.append(declare);
    }
    return item;
  }

  /* The frame the scene currently places, so the panel can say which hops
     sit outside it instead of letting them vanish. */
  function placedRefs() {
    const placed = new Set(model.entries.map((entry) => entry.ref));
    for (const layer of model.layerProjections || [])
      for (const entry of layer.projection?.entries || []) placed.add(entry.ref_id);
    return placed;
  }

  function render() {
    const drawn = view.paths;
    const box = $("paths-box");
    box.hidden = !drawn;
    if (!drawn) {
      closeDeclare();
      return;
    }
    const placed = placedRefs();
    $("paths-summary").textContent = drawn.summary || `${drawn.chains.length} paths`;
    const warnings = $("paths-warnings");
    warnings.textContent = "";
    for (const warning of drawn.warnings) warnings.append(el("li", "muted", warning));
    const chains = $("paths-chains");
    chains.textContent = "";
    if (!drawn.chains.length)
      chains.append(el("p", "empty-hint", "No chain joins these facts over the drawn planes."));
    for (const chain of drawn.chains) {
      const section = el("section", "path-chain");
      section.append(
        el(
          "h4",
          "path-chain-title",
          `Path ${chain.number} · ${chain.hops.length} step${chain.hops.length === 1 ? "" : "s"} · ${chain.proposed} proposed · confidence ${KMP_APP.pathModel.confidenceText(chain.confidence)}`,
        ),
      );
      const list = el("ol", "path-hops");
      for (const hop of chain.hops) list.append(hopItem(hop, placed));
      section.append(list);
      chains.append(section);
    }
    const avoided = $("paths-avoided");
    avoided.textContent = "";
    for (const hop of drawn.avoided) avoided.append(hopItem(hop, placed));
    $("paths-avoided-box").hidden = !drawn.avoided.length;
  }

  function openDeclare(hop) {
    declaring = hop;
    $("declare-hop").textContent = `${short(hop.from.excerpt || hop.from.ref, 90)} —${hop.rel || "untyped"}→ ${short(hop.to.excerpt || hop.to.ref, 90)} (Jev ${KMP_APP.pathModel.confidenceText(hop.confidence)}, ${hop.item_id})`;
    $("declare-why").value = "";
    $("declare-evidence").value = "";
    $("declare-confidence").value = "";
    $("declare-error").textContent = "";
    $("declare-call").hidden = true;
    $("declare-call").textContent = "";
    const dialog = $("declare-dialog");
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    $("declare-why").focus();
  }

  function closeDeclare() {
    declaring = null;
    const dialog = $("declare-dialog");
    if (dialog.open) dialog.close?.();
  }

  async function compose(event) {
    event.preventDefault();
    const { call, error } = KMP_APP.pathModel.declareCall(view.paths, declaring, {
      why: $("declare-why").value,
      evidence: $("declare-evidence").value,
      confidence: $("declare-confidence").value,
      actor: $("declare-actor").value,
    });
    $("declare-error").textContent = error || "";
    if (!call) return;
    const text = JSON.stringify(call, null, 2);
    $("declare-call").textContent = text;
    $("declare-call").hidden = false;
    let copied = false;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch (_) {
      // A page without clipboard permission still shows the exact call.
    }
    $("declare-error").textContent = copied
      ? "Copied. Give it to your agent: it runs the call, and nothing is written until it does."
      : "Copy the call above and give it to your agent; nothing is written until it runs it.";
  }

  function wire() {
    $("declare-form").addEventListener("submit", compose);
    $("declare-cancel").addEventListener("click", closeDeclare);
    $("paths-clear").addEventListener("click", () => {
      view.paths = null;
      render();
      KMP_APP.scene.requestDraw();
      KMP_APP.sync.clearPaths();
    });
  }

  return { render, wire, openDeclare, closeDeclare };
})();
