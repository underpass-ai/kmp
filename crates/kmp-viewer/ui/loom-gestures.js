/* KMP ChronoLoom — the gestures.
   The input adapter: pointer, wheel and keyboard on the loom canvas, and
   the navigator's pan/resize/select drags. Gestures translate motion into
   viewport moves and selections; they decide nothing about data.
   Exposes KMP_APP.gestures. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.gestures = (() => {
  const { model, view } = KMP_APP.state;
  const { $ } = KMP_APP.dom;
  const viewport = () => KMP_APP.viewport;
  const scene = () => KMP_APP.scene;

  /* ---------------- navigator gestures ----------------
     Pan the pane, resize its edges, cut a fresh window, click to center. */
  let navDrag = null;
  const NAV_HANDLE = 6;

  function wireNavigator() {
    $("nav-all").addEventListener("click", () => viewport().setWindow(view.full.t0, view.full.t1));
    $("nav-back").addEventListener("click", () => {
      const previous = view.windowStack.pop();
      if (previous) viewport().setWindow(previous[0], previous[1], false);
    });
    $("btn-fit").addEventListener("click", () => viewport().setWindow(view.full.t0, view.full.t1));

    $("nav-canvas").addEventListener("pointerdown", (event) => {
      $("nav-canvas").setPointerCapture(event.pointerId);
      const width = $("nav-canvas").clientWidth || 1;
      const span = view.full.t1 - view.full.t0;
      const xAt = (t) => ((t - view.full.t0) / span) * width;
      const x = event.offsetX;
      const a = xAt(view.t0);
      const b = xAt(view.t1);
      const windowed = view.t0 > view.full.t0 || view.t1 < view.full.t1;
      let mode = "select";
      if (windowed && Math.abs(x - a) <= NAV_HANDLE) mode = "resize-l";
      else if (windowed && Math.abs(x - b) <= NAV_HANDLE) mode = "resize-r";
      else if (windowed && x > a && x < b) mode = "pan";
      navDrag = { mode, x0: x, moved: false, t0: view.t0, t1: view.t1, remembered: false };
    });

    $("nav-canvas").addEventListener("pointermove", (event) => {
      if (!navDrag) return;
      const width = $("nav-canvas").clientWidth || 1;
      const span = view.full.t1 - view.full.t0;
      const x = event.offsetX;
      if (!navDrag.moved && Math.abs(x - navDrag.x0) <= 4) return;
      navDrag.moved = true;
      const msOf = (px) => (px / width) * span;
      if (!navDrag.remembered && navDrag.mode !== "select") {
        view.windowStack.push([navDrag.t0, navDrag.t1]);
        navDrag.remembered = true;
      }
      if (navDrag.mode === "pan") {
        const delta = msOf(x - navDrag.x0);
        const size = navDrag.t1 - navDrag.t0;
        let t0 = navDrag.t0 + delta;
        t0 = Math.max(view.full.t0, Math.min(view.full.t1 - size, t0));
        viewport().setWindow(t0, t0 + size, false);
      } else if (navDrag.mode === "resize-l") {
        viewport().setWindow(view.full.t0 + msOf(x), navDrag.t1, false);
      } else if (navDrag.mode === "resize-r") {
        viewport().setWindow(navDrag.t0, view.full.t0 + msOf(x), false);
      } else {
        // Painting a fresh selection — live preview via the window itself.
        if (!navDrag.remembered) {
          view.windowStack.push([navDrag.t0, navDrag.t1]);
          navDrag.remembered = true;
        }
        const lo = view.full.t0 + msOf(Math.min(navDrag.x0, x));
        const hi = view.full.t0 + msOf(Math.max(navDrag.x0, x));
        viewport().setWindow(lo, hi, false);
      }
    });

    $("nav-canvas").addEventListener("pointerup", (event) => {
      if (!navDrag) return;
      const drag = navDrag;
      navDrag = null;
      if (!drag.moved && drag.mode === "select") {
        // A click on the open line: carry the window there, centered.
        const width = $("nav-canvas").clientWidth || 1;
        const span = view.full.t1 - view.full.t0;
        const center = view.full.t0 + (event.offsetX / width) * span;
        const size = drag.t1 - drag.t0;
        let t0 = center - size / 2;
        t0 = Math.max(view.full.t0, Math.min(view.full.t1 - size, t0));
        viewport().setWindow(t0, t0 + size);
      }
    });
    $("nav-canvas").addEventListener("pointercancel", () => (navDrag = null));
  }

  /* ---------------- loom gestures ---------------- */

  let loomDrag = null;

  function wireLoom() {
    const canvas = scene().canvas();

    canvas.addEventListener("pointerdown", (event) => {
      canvas.setPointerCapture(event.pointerId);
      canvas.classList.add("dragging");
      loomDrag = { x0: event.offsetX, t0: view.t0, t1: view.t1, moved: false };
    });

    canvas.addEventListener("pointermove", (event) => {
      if (loomDrag) {
        const dx = event.offsetX - loomDrag.x0;
        if (loomDrag.moved || Math.abs(dx) > 4) {
          loomDrag.moved = true;
          const span = loomDrag.t1 - loomDrag.t0;
          const delta = (-dx / Math.max(1, canvas.clientWidth)) * span;
          let t0 = loomDrag.t0 + delta;
          t0 = Math.max(view.full.t0, Math.min(view.full.t1 - span, t0));
          viewport().setWindow(t0, t0 + span, false);
        }
        return;
      }
      if (view.overlays.length) {
        KMP_APP.panels.renderPulseLegend(viewport().tOf(event.offsetX));
      }
      const hit = scene().hitAt(event.offsetX, event.offsetY);
      scene().updateTooltip(hit, event.offsetX, event.offsetY);
    });

    canvas.addEventListener("pointerup", (event) => {
      canvas.classList.remove("dragging");
      if (!loomDrag) return;
      const drag = loomDrag;
      loomDrag = null;
      if (drag.moved) return;
      const hit = scene().hitAt(event.offsetX, event.offsetY);
      if (!hit) {
        view.selectedRef = null;
        KMP_APP.panels.renderDetailEmpty();
        scene().requestDraw();
        return;
      }
      if (hit.kind === "cluster") {
        // Open the weave: zoom into the bundle's span.
        const pad = Math.max(1000, (hit.t1 - hit.t0) * 0.35);
        viewport().setWindow(hit.t0 - pad, hit.t1 + pad);
        return;
      }
      if (hit.kind === "exemplar") {
        const ref = hit.exemplar && hit.exemplar.bundle_ref;
        if (ref && model.byRef.has(ref)) {
          KMP_APP.selection.selectEntry(ref);
        } else if (hit.exemplar) {
          const revision = hit.exemplar.revision == null ? "revision unavailable" : `revision ${hit.exemplar.revision}`;
          KMP_APP.dom.showError(`${hit.exemplar.operation} · ${hit.exemplar.about || "unknown bundle"} · ${revision}; temporal window preserved`);
        }
        return;
      }
      KMP_APP.selection.selectEntry(hit.ref);
    });

    canvas.addEventListener("pointerleave", () => {
      $("tooltip").hidden = true;
      KMP_APP.panels.renderPulseLegend();
    });

    canvas.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        const factor = Math.exp(event.deltaY * 0.0014);
        const span = (view.t1 - view.t0) * factor;
        const fullSpan = view.full.t1 - view.full.t0;
        const clampedSpan = Math.max(1000, Math.min(fullSpan, span));
        const anchor = viewport().tOf(event.offsetX);
        const ratio = (anchor - view.t0) / (view.t1 - view.t0);
        let t0 = anchor - clampedSpan * ratio;
        t0 = Math.max(view.full.t0, Math.min(view.full.t1 - clampedSpan, t0));
        viewport().setWindow(t0, t0 + clampedSpan, false);
      },
      { passive: false }
    );
  }

  function wireKeyboard() {
    addEventListener("keydown", (event) => {
      const target = event.target;
      if (target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")) return;
      const span = view.t1 - view.t0;
      if (event.key === "f" || event.key === "F") viewport().setWindow(view.full.t0, view.full.t1);
      else if (event.key === "ArrowLeft") viewport().setWindow(view.t0 - span * 0.25, view.t1 - span * 0.25, false);
      else if (event.key === "ArrowRight") viewport().setWindow(view.t0 + span * 0.25, view.t1 + span * 0.25, false);
      else if (event.key === "+" || event.key === "=") viewport().setWindow(view.t0 + span * 0.2, view.t1 - span * 0.2);
      else if (event.key === "-") viewport().setWindow(view.t0 - span * 0.3, view.t1 + span * 0.3);
    });
  }

  function wire() {
    wireNavigator();
    wireLoom();
    wireKeyboard();
  }

  return { wire };
})();
