/* WebGL relation adapter: shared line buffers, retained curves and arrowheads. */
"use strict";
KMP_APP.relationLines = (() => {
  const { THREE } = KMP_THREE;
  const steps = 40;
  class RelationLines {
    constructor(group) {
      this.group = group;
      this.paths = new Map();
      this.batches = new Map();
      this.arrows = new Map();
      this.arrowGeometry = new THREE.ConeGeometry(2.3, 7, 8);
    }
    path(key, a, b) {
      const positions = [a.x, a.y, a.z, b.x, b.y, b.z];
      const previous = this.paths.get(key);
      if (previous && positions.every((value, i) => value === previous.positions[i])) return previous;
      const bend = Math.min(85, Math.max(24, Math.abs(b.x - a.x) * 0.12));
      const curve = new THREE.CubicBezierCurve3(
        new THREE.Vector3(a.x, a.y, a.z + 2),
        new THREE.Vector3(a.x + (b.x - a.x) * 0.25, a.y + bend, a.z + 2),
        new THREE.Vector3(a.x + (b.x - a.x) * 0.75, b.y + bend, b.z + 2),
        new THREE.Vector3(b.x, b.y, b.z + 2),
      );
      const points = curve.getPoints(steps), vertices = new Float32Array(steps * 6);
      for (let i = 0; i < steps; i++) {
        points[i].toArray(vertices, i * 6);
        points[i + 1].toArray(vertices, i * 6 + 3);
      }
      const path = { positions, vertices, tip: curve.getPoint(0.96), tangent: curve.getTangent(0.96).normalize() };
      this.paths.set(key, path);
      return path;
    }
    update(edges, nodes, style) {
      const groups = new Map(), livePaths = new Set(), liveArrows = new Set();
      for (const edge of edges) {
        const a = nodes.get(edge.source), b = nodes.get(edge.target);
        if (!a || !b) continue;
        const key = `${edge.source} ${edge.rel} ${edge.target}`;
        const path = this.path(key, a, b), { color, selected } = style(edge);
        livePaths.add(key);
        const opacity = selected ? 0.85 : 0.18, batchKey = `${color}:${opacity}`;
        if (!groups.has(batchKey)) groups.set(batchKey, { color, opacity, paths: [] });
        groups.get(batchKey).paths.push(path);
        if (selected) {
          liveArrows.add(key);
          let arrow = this.arrows.get(key);
          if (!arrow) {
            arrow = new THREE.Mesh(this.arrowGeometry, new THREE.MeshBasicMaterial({ color }));
            this.arrows.set(key, arrow); this.group.add(arrow);
          }
          arrow.material.color.set(color);
          arrow.position.copy(path.tip);
          arrow.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), path.tangent);
        }
      }
      for (const [key, path] of groups) {
        let batch = this.batches.get(key);
        if (!batch) {
          const line = new THREE.LineSegments(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({
            color: path.color, transparent: true, opacity: path.opacity, depthWrite: false,
          }));
          batch = { line, paths: [] }; this.batches.set(key, batch); this.group.add(line);
        }
        if (path.paths.length === batch.paths.length && path.paths.every((value, i) => value === batch.paths[i])) continue;
        const size = path.paths.length * steps * 6, geometry = batch.line.geometry;
        let attribute = geometry.getAttribute("position");
        if (!attribute || attribute.array.length < size || attribute.array.length > size * 4) {
          // Dispose an old GPU buffer before replacing it; BufferGeometry does
          // not free a superseded attribute's WebGL allocation on setAttribute.
          geometry.dispose();
          attribute = new THREE.BufferAttribute(new Float32Array(size), 3);
          attribute.setUsage(THREE.DynamicDrawUsage);
          geometry.setAttribute("position", attribute);
        }
        for (let i = 0; i < path.paths.length; i++) attribute.array.set(path.paths[i].vertices, i * steps * 6);
        attribute.needsUpdate = true;
        geometry.setDrawRange(0, size / 3);
        geometry.computeBoundingSphere();
        batch.paths = path.paths;
      }
      for (const key of this.paths.keys()) if (!livePaths.has(key)) this.paths.delete(key);
      for (const [key, arrow] of this.arrows) if (!liveArrows.has(key)) {
        this.group.remove(arrow); arrow.material.dispose(); this.arrows.delete(key);
      }
      for (const [key, batch] of this.batches) if (!groups.has(key)) {
        this.group.remove(batch.line); batch.line.geometry.dispose(); batch.line.material.dispose(); this.batches.delete(key);
      }
    }
    clear() {
      for (const batch of this.batches.values()) {
        this.group.remove(batch.line); batch.line.geometry.dispose(); batch.line.material.dispose();
      }
      for (const arrow of this.arrows.values()) { this.group.remove(arrow); arrow.material.dispose(); }
      this.batches.clear(); this.arrows.clear(); this.paths.clear();
    }
    dispose() { this.clear(); this.arrowGeometry.dispose(); }
  }
  return { RelationLines };
})();
