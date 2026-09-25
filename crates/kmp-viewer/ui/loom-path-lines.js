/* WebGL adapter for drawn paths: declared hops solid, proposed hops dashed,
   avoided declarations dotted and faint, each labelled with its relation
   and, when it is a judgement, its confidence. Receives hop values from the
   path model; has no memory or transport port. */
"use strict";
KMP_APP.pathLines = (() => {
  const { THREE } = KMP_THREE;
  const { arc } = KMP_APP.relationLines;
  const STEPS = 48;
  const LIFT = 26;
  const dashes = {
    proposed: { dashSize: 7, gapSize: 5 },
    avoided: { dashSize: 2, gapSize: 5 },
  };
  class PathLines {
    constructor(group, container) {
      this.group = group;
      this.container = container;
      this.records = new Map();
      this.arrowGeometry = new THREE.ConeGeometry(2.8, 8, 8);
    }
    create(hop) {
      const dashed = dashes[hop.kind];
      const material = dashed
        ? new THREE.LineDashedMaterial({ transparent: true, depthWrite: false, ...dashed })
        : new THREE.LineBasicMaterial({ transparent: true, depthWrite: false });
      const line = new THREE.Line(new THREE.BufferGeometry(), material);
      const arrow = new THREE.Mesh(this.arrowGeometry, new THREE.MeshBasicMaterial({ transparent: true }));
      const element = document.createElement("span");
      element.className = `path-label path-${hop.kind}`;
      this.container.append(element);
      this.group.add(line, arrow);
      return { line, arrow, element, positions: null, label: { element, point: new THREE.Vector3(), type: "path" } };
    }
    remove(record) {
      this.group.remove(record.line, record.arrow);
      record.line.geometry.dispose(); record.line.material.dispose(); record.arrow.material.dispose();
      record.element.remove();
    }
    /* Draws `hops` between the placed `nodes`; returns the labels the scene
       projects onto the screen with its other labels. */
    update(hops, nodes, style) {
      const live = new Set(), labels = [];
      let order = 0;
      for (const hop of hops) {
        const a = nodes.get(hop.source), b = nodes.get(hop.target);
        if (!a || !b) continue;
        // Each hop rides its own height, so two chains through the same
        // facts never draw one proposal on top of another.
        const lift = LIFT + 16 * order++;
        live.add(hop.key);
        let record = this.records.get(hop.key);
        if (!record) { record = this.create(hop); this.records.set(hop.key, record); }
        const positions = [a.x, a.y, a.z, b.x, b.y, b.z, lift];
        if (!record.positions || positions.some((value, i) => value !== record.positions[i])) {
          const curve = arc(a, b, lift);
          record.line.geometry.setFromPoints(curve.getPoints(STEPS));
          record.line.geometry.computeBoundingSphere();
          if (record.line.material.isLineDashedMaterial) record.line.computeLineDistances();
          record.arrow.position.copy(curve.getPoint(0.95));
          record.arrow.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), curve.getTangent(0.95).normalize());
          record.label.point.copy(curve.getPoint(0.5));
          record.positions = positions;
        }
        const { color, opacity, text } = style(hop);
        record.line.material.color.set(color);
        record.line.material.opacity = opacity;
        record.arrow.material.color.set(color);
        record.arrow.material.opacity = opacity;
        if (record.element.textContent !== text) record.element.textContent = text;
        record.element.style.setProperty("--path-color", color);
        labels.push(record.label);
      }
      for (const [key, record] of this.records) if (!live.has(key)) { this.remove(record); this.records.delete(key); }
      return labels;
    }
    clear() { for (const record of this.records.values()) this.remove(record); this.records.clear(); }
    dispose() { this.clear(); this.arrowGeometry.dispose(); }
  }
  return { PathLines };
})();
