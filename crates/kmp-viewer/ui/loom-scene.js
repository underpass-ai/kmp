/* Scene composition adapter: the use cases see this port, never Three.js. */
"use strict";
KMP_APP.scene = (() => {
  const { model, view } = KMP_APP.state;
  const { $ } = KMP_APP.dom;
  const canvas = () => $("loom-canvas");
  let renderer = null,
    frame = null,
    current = null;
  function sceneState() {
    return {
      mode: view.sceneMode,
      gap: view.layerGap,
      opacity: view.layerOpacity,
      selected: view.selectedRef,
      relations: view.relationMode,
      relationClasses: view.relationClasses,
      clock: view.clock,
      from: view.t0,
      to: view.t1,
      scale: view.lensMode,
      dimmedKinds: view.dimmedKinds,
      searchHits: view.searchHits,
      hiddenLanes: view.hiddenLanes,
      trace: view.trace,
    };
  }
  function draw() {
    frame = null;
    if (!renderer) return;
    const layers = [
      {
        about: model.about || "Memory",
        projection: model.projection || {},
        lod: model.currentLod,
      },
      ...(model.layerProjections || []),
    ];
    const state = sceneState();
    current = KMP_APP.sceneModel.layout(layers, state, (time) =>
      KMP_APP.viewport.lens().toRatio(time),
    );
    renderer.update(current, state);
    $("lod-chip").textContent = model.currentLod;
    $("lod-mode").value = view.requestedLod || "";
    $("scene-empty").hidden = current.nodes.length > 0;
    $("scene-count").textContent =
      `${layers.length} about${layers.length === 1 ? "" : "s"} · ${current.nodes.length} ${model.currentLod === "moment" ? "memories" : "aggregates"}`;
  }
  function requestDraw() {
    if (!frame) frame = requestAnimationFrame(draw);
  }
  async function picked(ref) {
    const mark = current?.nodes.find((node) => node.entry.ref === ref)?.entry;
    if (!mark) return;
    if (mark.about !== model.about) await KMP_APP.layers.activate(mark.about);
    if (mark.aggregate) {
      const pad = Math.max(1000, (mark.to - mark.from) * 0.15);
      KMP_APP.viewport.setWindow(mark.from - pad, mark.to + pad);
    } else {
      await KMP_APP.selection.selectEntry(ref);
    }
  }
  function hover(ref, event) {
    const tip = $("tooltip");
    const mark = current?.nodes.find((node) => node.entry.ref === ref)?.entry;
    tip.hidden = !mark;
    if (!mark) return;
    tip.textContent = `${mark.about} · ${mark.text}`;
    const bounds = $("scene-world").getBoundingClientRect();
    tip.style.left = `${Math.min(bounds.width - 280, Math.max(8, event.clientX - bounds.left + 12))}px`;
    tip.style.top = `${Math.max(8, event.clientY - bounds.top - 55)}px`;
  }
  async function setup() {
    renderer = new KMP_APP.three.MemoryScene(
      $("scene-world"),
      canvas(),
      picked,
      hover,
    );
    renderer.controls.addEventListener("start", () =>
      KMP_APP.control.humanGesture(),
    );
    requestDraw();
  }
  function wire() {
    KMP_APP.theme.wire();
    for (const button of document.querySelectorAll("[data-scene-mode]")) {
      button.addEventListener("click", () => {
        view.sceneMode = button.dataset.sceneMode;
        for (const item of document.querySelectorAll("[data-scene-mode]"))
          item.setAttribute("aria-pressed", String(item === button));
        KMP_APP.control.humanGesture();
        requestDraw();
      });
    }
    $("camera-reset").addEventListener("click", () => {
      renderer?.resetCamera();
      KMP_APP.control.humanGesture();
    });
    for (const button of document.querySelectorAll("[data-camera]"))
      button.addEventListener("click", () => {
        renderer?.nudge(button.dataset.camera);
        KMP_APP.control.humanGesture();
      });
    $("lod-mode").addEventListener("change", (event) => {
      view.requestedLod = event.target.value || null;
      KMP_APP.data.loadProjection();
      KMP_APP.sync.reportView();
    });
    $("layer-gap").addEventListener("input", (event) => {
      view.layerGap = Number(event.target.value);
      requestDraw();
    });
    $("layer-opacity").addEventListener("input", (event) => {
      view.layerOpacity = Number(event.target.value);
      requestDraw();
    });
    $("relation-mode").addEventListener("change", (event) => {
      view.relationMode = event.target.value;
      requestDraw();
    });
  }
  return {
    canvas,
    setup,
    wire,
    requestDraw,
    drawNavigator: () => KMP_APP.navigator.draw(),
    applyTheme: () => KMP_APP.theme.applyTheme(),
    palette: () => KMP_APP.theme.palette(),
    kindColor: (kind) => KMP_APP.theme.kindColor(kind),
    classColor: (name) => KMP_APP.theme.classColor(name),
    pulseColors: () => KMP_APP.theme.pulseColors(),
  };
})();
