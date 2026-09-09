/* Evidence sidebar DOM adapter: accessible selection and the clock prism. */
"use strict";
KMP_APP.evidence = (() => {
  const { model, view } = KMP_APP.state;
  const { $, el, fmtMsFull } = KMP_APP.dom;
  function renderPicker() {
    const picker = $("memory-picker"),
      selected = view.selectedRef;
    picker.replaceChildren(
      el(
        "option",
        "",
        model.currentLod === "moment"
          ? "Choose a memory"
          : "Choose Memories detail to inspect",
      ),
    );
    picker.firstChild.value = "";
    for (const entry of model.entries) {
      const option = el(
        "option",
        "",
        `${entry.kind} · ${entry.text.slice(0, 100)}`,
      );
      option.value = entry.ref;
      picker.append(option);
    }
    picker.value = selected || "";
    picker.disabled = !model.entries.length;
  }
  function wire() {
    $("memory-picker").addEventListener("change", (event) => {
      if (event.target.value) KMP_APP.selection.selectEntry(event.target.value);
    });
  }
  function renderPrism(m) {
    const box = $("prism");
    box.textContent = "";
    const prism = KMP_LOOM.prism(m);
    const palette = KMP_APP.scene.palette();
    const rails = [
      ["reality", prism.rails.occurred, "occurred"],
      ["perception", prism.rails.observed, "observed"],
      ["persistence", prism.rails.ingested, "ingested"],
    ];
    const span = prism.span;
    const posOf = (t) =>
      span ? ((t - span.t0) / (span.t1 - span.t0)) * 100 : 50;
    const dots = [];
    for (const [name, t] of rails) {
      const row = el("div", "prism-rail");
      row.append(el("span", "prism-name", name));
      const track = el("span", "prism-track");
      track.append(el("span", "prism-line"));
      if (t !== null) {
        const dot = el("span", "prism-dot");
        dot.style.left = `${posOf(t)}%`;
        dot.style.background =
          name === "reality"
            ? palette.cls.causal
            : name === "perception"
              ? palette.cls.constraint
              : palette.cls.evidential;
        track.append(dot);
        dots.push(posOf(t));
        row.append(track, el("span", "prism-when mono", fmtMsFull(t).slice(5)));
      } else {
        row.append(track, el("span", "prism-absent", "not recorded"));
      }
      box.append(row);
    }
    // The thread: occurred → ingested, the distance between "it was true"
    // and "KMP knew it".
    if (dots.length >= 2) {
      const thread = el("div", "prism-rail");
      thread.append(el("span", "prism-name", "thread"));
      const track = el("span", "prism-track");
      const line = el("span", "prism-thread");
      const lo = Math.min(...dots);
      const hi = Math.max(...dots);
      line.style.left = `${lo}%`;
      line.style.width = `${Math.max(1, hi - lo)}%`;
      track.append(line);
      thread.append(track, el("span", "prism-when", ""));
      box.append(thread);
    }
    if (prism.rails.validity) {
      const row = el("div", "prism-rail");
      row.append(el("span", "prism-name", "validity"));
      const track = el("span", "prism-track");
      track.append(el("span", "prism-line"));
      const band = el("span", "prism-band");
      const from = prism.rails.validity.from;
      const until = prism.rails.validity.until;
      band.style.left = `${from !== null ? posOf(from) : 0}%`;
      band.style.width = `${Math.max(4, (until !== null ? posOf(until) : 100) - (from !== null ? posOf(from) : 0))}%`;
      band.style.background = palette.accent;
      track.append(band);
      row.append(
        track,
        el(
          "span",
          "prism-when mono",
          until === null ? "open" : fmtMsFull(until).slice(5),
        ),
      );
      box.append(row);
    }
    for (const order of prism.order) {
      const row = el("div", "prism-rail");
      row.append(el("span", "prism-name", "order"));
      row.append(
        el(
          "span",
          "mono muted",
          `${order.dimension}=${order.value}${order.sequence !== null ? " #" + order.sequence : ""}${order.rank !== null ? " · rank " + order.rank : ""}`,
        ),
      );
      box.append(row);
    }
  }

  return { renderPrism, renderPicker, wire };
})();
