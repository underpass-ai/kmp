/* Retained about planes and their DOM labels. Coordinates are local to each plane. */
"use strict";
KMP_APP.planes = (() => {
  const { THREE } = KMP_THREE;
  const darkColors = ["#af95f1", "#64bba1", "#76a4d1", "#c99abb", "#d3b773"];
  const lightColors = ["#7954b4", "#287d65", "#416f9d", "#97537f", "#896922"];
  class Planes {
    constructor(group, container) {
      this.group = group;
      this.container = container;
      this.records = new Map();
      this.date = new Intl.DateTimeFormat("en-GB", { day: "2-digit", month: "short", timeZone: "UTC" });
      this.leaders = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      this.leaders.classList.add("plane-leaders");
      container.append(this.leaders);
    }
    create(about) {
      const group = new THREE.Group();
      this.group.add(group);
      const panel = new THREE.Mesh(new THREE.PlaneGeometry(940, 280), new THREE.MeshBasicMaterial({
        side: THREE.DoubleSide, transparent: true, depthWrite: false,
      }));
      group.add(panel);
      const lines = [];
      const line = (points, opacity, grid = false) => {
        const mesh = new THREE.Line(new THREE.BufferGeometry().setFromPoints(points.map(p => new THREE.Vector3(...p))),
          new THREE.LineBasicMaterial({ transparent: true, opacity, depthWrite: false }));
        group.add(mesh); lines.push({ mesh, grid });
      };
      line([[-470, 140, 0], [470, 140, 0], [470, -140, 0], [-470, -140, 0], [-470, 140, 0]], 0.38);
      line([[-470, 140, 0], [-470, -140, 0]], 0.9);
      for (let i = 0; i <= 8; i++) {
        const x = -410 + 820 * i / 8;
        line([[x, 102, 0.1], [x, -108, 0.1]], 0.14, true);
      }
      line([[-410, -116, 0.5], [410, -116, 0.5]], 0.5);
      const element = document.createElement("div"), title = document.createElement("span"), subtitle = document.createElement("small");
      element.className = "plane-label"; title.textContent = about;
      element.append(title, subtitle); this.container.append(element);
      const leader = document.createElementNS("http://www.w3.org/2000/svg", "line");
      this.leaders.append(leader);
      const label = { element, leader, point: new THREE.Vector3(), type: "plane" };
      const ticks = Array.from({ length: 4 }, () => {
        const element = document.createElement("span"); element.className = "axis-label";
        this.container.append(element);
        return { element, point: new THREE.Vector3(), type: "axis" };
      });
      return { group, panel, lines, subtitle, label, ticks };
    }
    remove(record) {
      this.group.remove(record.group);
      record.group.traverse(object => { object.geometry?.dispose(); object.material?.dispose(); });
      record.label.element.remove(); record.label.leader.remove();
      record.ticks.forEach(tick => tick.element.remove());
    }
    update(layout, state, light) {
      const active = new Set(), labels = [], counts = new Map();
      for (const node of layout.nodes) counts.set(node.entry.about, (counts.get(node.entry.about) || 0) + 1);
      for (const plane of layout.planes) {
        active.add(plane.about);
        let record = this.records.get(plane.about);
        if (!record) { record = this.create(plane.about); this.records.set(plane.about, record); }
        const color = (light ? lightColors : darkColors)[plane.index % 5];
        record.group.position.set(0, plane.y, plane.z);
        record.panel.material.color.set(light ? "#c0d6c8" : "#314d40");
        record.panel.material.opacity = state.opacity / 100;
        for (const line of record.lines) line.mesh.material.color.set(line.grid ? (light ? "#60816d" : "#6f9880") : color);
        record.label.element.style.setProperty("--plane-color", color);
        record.label.leader.setAttribute("stroke", color);
        const caption = plane.caption || `${counts.get(plane.about) || 0} memories`;
        if (record.subtitle.textContent !== caption) record.subtitle.textContent = caption;
        record.label.point.set(-470, plane.y + 140, plane.z);
        labels.push(record.label);
        record.ticks.forEach((tick, i) => {
          const rank = layout.times.length ? Math.round((layout.times.length - 1) * i / 3) : 0;
          const time = state.scale === "elapsed" ? state.from + (state.to - state.from) * i / 3 : layout.times[rank];
          const visible = (plane.index === 0 || state.mode === "flat") && time !== undefined;
          tick.element.hidden = !visible;
          if (!visible) return;
          const text = this.date.format(time);
          if (tick.element.textContent !== text) tick.element.textContent = text;
          tick.point.set(state.scale === "elapsed" ? -410 + 820 * i / 3 : layout.timeX(time), plane.y - 118, plane.z);
          labels.push(tick);
        });
      }
      for (const [about, record] of this.records) if (!active.has(about)) { this.remove(record); this.records.delete(about); }
      return labels;
    }
    clear() { for (const record of this.records.values()) this.remove(record); this.records.clear(); }
    dispose() { this.clear(); this.leaders.remove(); }
  }
  return { Planes };
})();
