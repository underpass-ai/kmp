/* DOM adapter for about planes and ordinary label predicates. */
"use strict";
KMP_APP.catalogue = (() => {
  const { model, view } = KMP_APP.state;
  const { $, el } = KMP_APP.dom;
  function renderAbouts() {
    const list = $("about-list");
    list.replaceChildren();
    for (const about of [...new Set([model.about, ...view.layerAbouts, ...model.abouts])].filter(
      Boolean,
    )) {
      const row = el("li", "about-row");
      const label = el("label"),
        check = el("input");
      check.type = "checkbox";
      check.checked = about === model.about || view.layerAbouts.includes(about);
      check.disabled = about === model.about;
      check.addEventListener("change", () =>
        KMP_APP.layers.set(
          check.checked
            ? [...view.layerAbouts, about]
            : view.layerAbouts.filter((name) => name !== about),
        ),
      );
      label.append(check, el("span", "about-title", about));
      row.append(label);
      if (about === model.about) row.append(el("small", "", "Active context"));
      else {
        const open = el("button", "quiet", "Explore");
        open.addEventListener("click", () => KMP_APP.layers.activate(about));
        row.append(open);
      }
      list.append(row);
    }
  }
  function renderLabels() {
    const list = $("lane-list");
    list.replaceChildren();
    const search = $("label-search").value.trim().toLowerCase();
    const labels = model.projection?.labels || [];
    for (const label of labels) {
      const key = label.dimension,
        name = label.value,
        scope = label.scope_id;
      if (!name || !(key + "=" + name).toLowerCase().includes(search)) continue;
      const item = el("li"),
        button = el("button", "label-option");
      button.append(
        el("span", "", name),
        el("small", "mono", `${key} · ${label.in_range}/${label.entries}`),
      );
      button.title = `${key}=${name}: ${label.in_range} in this window, ${label.entries} in this about`;
      button.addEventListener("click", () =>
        KMP_APP.data.setSelectors([
          ...view.selectors.filter((selector) => selector.key !== key),
          { key, op: "in", values: [scope] },
        ]),
      );
      item.append(button);
      list.append(item);
    }
    if (!list.children.length)
      list.append(
        el(
          "li",
          "muted",
          search ? "No matching labels" : "No labels in this context",
        ),
      );
  }
  function wire() {
    $("label-search").addEventListener("input", renderLabels);
  }
  return { renderAbouts, renderLabels, wire };
})();
