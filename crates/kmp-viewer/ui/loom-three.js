/* Three.js adapter. Receives a scene value; has no memory or transport port. */
KMP_APP.three = (() => {
  const { THREE, OrbitControls } = KMP_THREE;
  const { fittedCamera } = KMP_APP.camera;
  const kindColors = {
    decision: "#91a9ee",
    constraint: "#d5b571",
    observation: "#7ebccc",
    success_path: "#68c4a0",
    error_path: "#e78e98",
    semantic_delta: "#c99abc",
    derived_value: "#b195ed",
  };
  const lightKindColors = {
    decision: "#34599c",
    constraint: "#886316",
    observation: "#276e7c",
    success_path: "#24785b",
    error_path: "#a44354",
    semantic_delta: "#925178",
    derived_value: "#72499c",
  };
  /* Declared hops read as relations, a judge's proposals as a warmer dashed
     suggestion, avoided declarations as a faint warning. */
  const pathColors = {
    dark: { declared: "#68c4a0", proposed: "#f0b35a", avoided: "#e78e98" },
    light: { declared: "#24785b", proposed: "#9a5b00", avoided: "#a44354" },
  };
  class MemoryScene {
    constructor(container, canvas, onSelect, onHover) {
      this.container = container;
      this.onSelect = onSelect;
      this.onHover = onHover;
      this.renderer = new THREE.WebGLRenderer({
        canvas,
        antialias: true,
        alpha: false,
      });
      this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
      this.renderer.outputColorSpace = THREE.SRGBColorSpace;
      this.scene = new THREE.Scene();
      this.camera = new THREE.PerspectiveCamera(38, 1, 1, 10000);
      this.camera.position.set(440, 370, 1200);
      this.controls = new OrbitControls(this.camera, canvas);
      this.controls.enableDamping = false;
      this.controls.minDistance = 130;
      this.controls.maxDistance = 4200;
      this.controls.minPolarAngle = 0.04;
      this.controls.maxPolarAngle = Math.PI - 0.04;
      this.onControlsChange = () => {
        if (this.controlFrame == null) this.controlFrame = requestAnimationFrame(() => {
          this.controlFrame = null; this.draw();
        });
      };
      this.controls.addEventListener("change", this.onControlsChange);
      this.scene.add(new THREE.AmbientLight(0xffffff, 2));
      const light = new THREE.DirectionalLight(0xffffff, 3);
      light.position.set(200, 600, 900);
      this.scene.add(light);
      this.group = new THREE.Group();
      this.scene.add(this.group);
      this.raycaster = new THREE.Raycaster();
      this.pointer = new THREE.Vector2();
      this.nodeMeshes = new Map();
      this.meshes = [];
      this.labels = [];
      this.geometry = {
        decision: new THREE.OctahedronGeometry(5.5),
        constraint: new THREE.BoxGeometry(8, 8, 8),
        normal: new THREE.SphereGeometry(4.2, 14, 10),
      };
      this.worldPoint = new THREE.Vector3();
      this.relationLines = new KMP_APP.relationLines.RelationLines(this.group);
      this.pathLines = new KMP_APP.pathLines.PathLines(this.group, document.getElementById("scene-labels"));
      this.planes = new KMP_APP.planes.Planes(this.group, document.getElementById("scene-labels"));
      this.ring = new THREE.Mesh(new THREE.RingGeometry(9, 10, 36), new THREE.MeshBasicMaterial({
        side: THREE.DoubleSide, transparent: true, opacity: 0.9,
      }));
      this.ring.userData.marker = true;
      this.ring.visible = false;
      this.group.add(this.ring);
      this.onPointerDown = (e) => {
        this.down = { x: e.clientX, y: e.clientY };
      };
      canvas.addEventListener("pointerdown", this.onPointerDown);
      this.onPointerUp = (e) => {
        if (
          this.down &&
          Math.hypot(e.clientX - this.down.x, e.clientY - this.down.y) < 5
        ) {
          const hit = this.pick(e);
          if (hit) this.onSelect(hit.userData.ref);
        }
        this.down = null;
      };
      this.onPointerMove = (e) => {
        if (e.buttons) return;
        this.hoverEvent = { clientX: e.clientX, clientY: e.clientY };
        if (this.hoverFrame == null) this.hoverFrame = requestAnimationFrame(() => {
          this.hoverFrame = null;
          const event = this.hoverEvent;
          this.hoverEvent = null;
          const hit = this.pick(event);
          canvas.style.cursor = hit ? "pointer" : "grab";
          this.onHover(hit?.userData.ref, event);
        });
      };
      this.onPointerLeave = () => {
        if (this.hoverFrame != null) cancelAnimationFrame(this.hoverFrame);
        this.hoverFrame = null; this.hoverEvent = null;
        this.onHover(null);
      };
      canvas.addEventListener("pointerup", this.onPointerUp);
      canvas.addEventListener("pointermove", this.onPointerMove);
      canvas.addEventListener("pointerleave", this.onPointerLeave);
      this.resizeObserver = new ResizeObserver(() => this.resize());
      this.resizeObserver.observe(container);
      this.resize();
    }
    pick(event) {
      const rect = this.renderer.domElement.getBoundingClientRect();
      this.pointer.set(
        ((event.clientX - rect.left) / rect.width) * 2 - 1,
        (-(event.clientY - rect.top) / rect.height) * 2 + 1,
      );
      this.raycaster.setFromCamera(this.pointer, this.camera);
      return this.raycaster.intersectObjects(this.meshes, false)[0]?.object;
    }
    fitFlatWidth() {
      const aspect =
        this.container.clientWidth / Math.max(1, this.container.clientHeight);
      this.worldXScale =
        this.mode === "flat"
          ? Math.max(
              1,
              (aspect * ((this.layout?.planes.length || 1) * 310 + 80)) / 1120,
            )
          : 1;
      this.group.scale.x = this.worldXScale;
      for (const mark of this.meshes)
        mark.scale.x = mark.scale.y / this.worldXScale;
      this.group.children
        .filter((o) => o.userData.marker)
        .forEach((o) => (o.scale.x = 1 / this.worldXScale));
    }
    resize() {
      this.fitFlatWidth();
      const w = Math.max(1, this.container.clientWidth),
        h = Math.max(1, this.container.clientHeight);
      this.renderer.setSize(w, h, false);
      if (this.camera.isPerspectiveCamera) this.camera.aspect = w / h;
      else {
        const size = this.flatSize || 700;
        this.camera.left = (-size * w) / h / 2;
        this.camera.right = (size * w) / h / 2;
        this.camera.top = size / 2;
        this.camera.bottom = -size / 2;
      }
      this.camera.updateProjectionMatrix();
      this.draw();
    }
    clear() {
      this.relationLines.clear();
      this.pathLines.clear();
      this.planes.clear();
      for (const mesh of this.nodeMeshes.values()) { this.group.remove(mesh); mesh.material.dispose(); }
      this.nodeMeshes.clear(); this.meshes = []; this.labels = [];
      this.ring.visible = false;
    }
    dispose() {
      if (!this.renderer) return;
      const canvas = this.renderer.domElement;
      this.onPointerLeave();
      canvas.removeEventListener("pointerdown", this.onPointerDown);
      canvas.removeEventListener("pointerup", this.onPointerUp);
      canvas.removeEventListener("pointermove", this.onPointerMove);
      canvas.removeEventListener("pointerleave", this.onPointerLeave);
      this.resizeObserver.disconnect();
      if (this.controlFrame != null) cancelAnimationFrame(this.controlFrame);
      this.controlFrame = null;
      this.controls.removeEventListener("change", this.onControlsChange);
      this.controls.dispose();
      this.clear(); this.planes.dispose(); this.relationLines.dispose(); this.pathLines.dispose();
      for (const geometry of Object.values(this.geometry)) geometry.dispose();
      this.ring.geometry.dispose(); this.ring.material.dispose();
      this.group.clear(); this.renderer.dispose(); this.renderer = null;
    }
    update(layout, state) {
      const modeChanged = this.mode !== state.mode;
      const layersChanged = this.layout?.planes.length !== layout.planes.length;
      this.mode = state.mode; this.layout = layout; this.state = state;
      const style = getComputedStyle(document.documentElement),
        bg = style.getPropertyValue("--canvas").trim(),
        light = document.documentElement.dataset.theme === "light";
      this.renderer.setClearColor(bg);
      if (!this.scene.background) this.scene.background = new THREE.Color(bg);
      else this.scene.background.set(bg);
      this.labels = this.planes.update(layout, state, light);
      const nodeByRef = new Map(layout.nodes.map(node => [node.entry.ref, node]));
      this.ring.visible = false;
      for (const node of layout.nodes) {
        const selected = node.entry.ref === state.selected,
          linked = layout.neighbors.has(node.entry.ref),
          color = (light ? lightKindColors : kindColors)[node.entry.kind] || kindColors.observation;
        const faded = node.dimmed || (state.relations === "selection" && layout.neighbors.size > 0 && !selected && !linked);
        let mesh = this.nodeMeshes.get(node.entry.ref);
        if (!mesh) {
          mesh = new THREE.Mesh(this.geometry.normal, new THREE.MeshStandardMaterial({ roughness: 0.5, metalness: 0.12 }));
          mesh.userData.ref = node.entry.ref;
          mesh.userData.baseScale = new THREE.Vector3();
          this.nodeMeshes.set(node.entry.ref, mesh); this.group.add(mesh);
        }
        mesh.geometry = node.entry.aggregate ? this.geometry.constraint : this.geometry[node.entry.kind] || this.geometry.normal;
        const material = mesh.material;
        material.color.set(color); material.emissive.set(color);
        material.emissiveIntensity = selected ? 0.7 : 0.1;
        if (material.transparent !== !!faded) { material.transparent = !!faded; material.needsUpdate = true; }
        material.opacity = node.dimmed ? 0.18 : faded ? (light ? 0.85 : 0.64) : 1;
        mesh.position.set(node.x, node.y, node.z);
        mesh.scale.setScalar(1);
        if (node.entry.aggregate) mesh.scale.set(2.4, 1.2, 0.3);
        if (selected) mesh.scale.setScalar(1.5);
        mesh.userData.baseScale.copy(mesh.scale);
        if (selected) {
          this.ring.visible = true; this.ring.material.color.set(color);
          this.ring.position.set(node.x, node.y, node.z + 0.3);
          this.ring.userData.ref = node.entry.ref;
        }
      }
      for (const [ref, mesh] of this.nodeMeshes) if (!nodeByRef.has(ref)) {
        this.group.remove(mesh); mesh.material.dispose(); this.nodeMeshes.delete(ref);
      }
      this.meshes = Array.from(this.nodeMeshes.values());
      this.relationLines.update(layout.shownRelations, nodeByRef, edge => ({
        selected: edge.source === state.selected || edge.target === state.selected ||
          state.trace?.edgeKeys.has(`${edge.source} ${edge.rel} ${edge.target}`),
        color: edge.rel === "supersedes" ? (light ? "#8e457b" : "#d69dc9") : KMP_APP.theme.classColor(edge.class),
      }));
      const colors = pathColors[light ? "light" : "dark"];
      this.labels.push(...this.pathLines.update(layout.pathHops || [], nodeByRef, hop => ({
        color: colors[hop.kind],
        opacity: hop.kind === "avoided" ? 0.6 : 0.95,
        text: KMP_APP.pathModel.hopLabel(hop),
      })));
      this.fitFlatWidth();
      if (modeChanged || layersChanged) this.resetCamera();
      else this.draw();
    }
    resetCamera() {
      const aspect =
        this.container.clientWidth / Math.max(1, this.container.clientHeight);
      const fitted = fittedCamera(
        this.layout || { planes: [] },
        this.mode,
        aspect,
        this.worldXScale,
      );
      this.camera = fitted.camera;
      this.flatSize = fitted.flatSize;
      this.controls.object = this.camera;
      this.controls.target.set(0, 0, 0);
      this.controls.enableRotate = this.mode !== "flat";
      this.controls.mouseButtons.LEFT =
        this.mode === "flat" ? THREE.MOUSE.PAN : THREE.MOUSE.ROTATE;
      this.controls.touches.ONE =
        this.mode === "flat" ? THREE.TOUCH.PAN : THREE.TOUCH.ROTATE;
      this.controls.update();
      this.resize();
    }

    nudge(direction) {
      if (direction === "in" || direction === "out") {
        const factor = direction === "in" ? 0.85 : 1.18;
        if (this.camera.isOrthographicCamera) {
          this.camera.zoom = Math.min(
            12,
            Math.max(0.2, this.camera.zoom / factor),
          );
          this.camera.updateProjectionMatrix();
        } else
          this.camera.position
            .sub(this.controls.target)
            .multiplyScalar(factor)
            .add(this.controls.target);
      } else if (this.mode === "3d") {
        const offset = this.camera.position.clone().sub(this.controls.target),
          s = new THREE.Spherical().setFromVector3(offset);
        if (direction === "left") s.theta -= 0.15;
        if (direction === "right") s.theta += 0.15;
        if (direction === "up") s.phi = Math.max(0.05, s.phi - 0.12);
        if (direction === "down")
          s.phi = Math.min(Math.PI - 0.05, s.phi + 0.12);
        this.camera.position.copy(
          new THREE.Vector3().setFromSpherical(s).add(this.controls.target),
        );
      } else {
        const delta = new THREE.Vector3(
          direction === "left" ? -40 : direction === "right" ? 40 : 0,
          direction === "up" ? 40 : direction === "down" ? -40 : 0,
          0,
        );
        this.camera.position.add(delta);
        this.controls.target.add(delta);
      }
      this.controls.update();
      this.draw();
    }
    draw() {
      if (this.controlFrame != null) cancelAnimationFrame(this.controlFrame);
      this.controlFrame = null;
      if (!this.renderer) return;
      this.camera.updateMatrixWorld();
      // Keep marks pickable when several planes fit into a short viewport.
      // Positions and time spacing stay in world units; only glyph size has
      // a minimum screen footprint, in both camera modes.
      const height = Math.max(1, this.container.clientHeight);
      const sizes = new Map();
      for (const mark of this.meshes) {
        const unitsPerPixel = this.camera.isOrthographicCamera
          ? (this.camera.top - this.camera.bottom) / this.camera.zoom / height
          : (2 *
              this.camera.position.distanceTo(
                mark.getWorldPosition(this.worldPoint),
              ) *
              Math.tan(THREE.MathUtils.degToRad(this.camera.fov / 2))) /
            height;
        const size = Math.max(1, (3.5 * unitsPerPixel) / 4.2);
        const base = mark.userData.baseScale;
        mark.scale.set(
          (base.x * size) / this.worldXScale,
          base.y * size,
          base.z * size,
        );
        sizes.set(mark.userData.ref, size);
      }
      for (const marker of this.group.children.filter(
        (child) => child.userData.marker,
      )) {
        const size = sizes.get(marker.userData.ref) || 1;
        marker.scale.set(size / this.worldXScale, size, size);
      }
      this.renderer.render(this.scene, this.camera);
      const w = this.container.clientWidth,
        h = this.container.clientHeight;
      const projected = this.labels.map((label) => {
        const point = label.point.clone();
        point.x *= this.worldXScale;
        const p = point.project(this.camera);
        return {
          ...label,
          x: ((p.x + 1) * w) / 2,
          y: ((1 - p.y) * h) / 2,
          outside: p.z < -1 || p.z > 1,
        };
      });
      const planes = projected
        .filter((l) => l.type === "plane")
        .sort((a, b) => a.y - b.y);
      for (const label of planes) label.element.hidden = false;
      for (const label of planes) { label.width = label.element.offsetWidth; label.height = label.element.offsetHeight || 38; }
      const labelWidth = Math.max(
        0,
        ...planes.map((l) => l.width),
      );
      const left = Math.max(
        8,
        Math.min(
          w - labelWidth - 8,
          Math.min(...planes.map((l) => l.x)) - labelWidth - 18,
        ),
      );
      let previousBottom = 0;
      for (const label of planes) {
        const height = label.height,
          y = Math.max(
            height / 2 + 8,
            previousBottom + height / 2 + 6,
            Math.min(h - height / 2 - 8, label.y),
          );
        const outside =
          label.outside || label.x < 0 || label.x > w || y + height / 2 > h;
        label.element.hidden = outside;
        label.leader.style.display = outside ? "none" : "";
        if (!outside) {
          label.element.style.left = left + "px";
          label.element.style.top = y + "px";
          previousBottom = y + height / 2;
          label.leader.setAttribute("x1", left + label.width + 4);
          label.leader.setAttribute("y1", y);
          label.leader.setAttribute("x2", label.x);
          label.leader.setAttribute("y2", label.y);
        }
      }
      const hops = [];
      for (const label of projected.filter((l) => l.type === "path")) {
        const outside = label.outside || label.x < 0 || label.x > w || label.y < 0 || label.y > h;
        // A label that would cover another steps aside rather than vanish:
        // a proposed hop without its type and confidence is not readable.
        const clash = (y) => hops.some((o) => Math.abs(o.x - label.x) < 90 && Math.abs(o.y - y) < 16);
        for (let step = 0; step < 4 && clash(label.y); step++) label.y += 17;
        const overlap = clash(label.y);
        label.element.hidden = outside || overlap;
        if (!outside && !overlap) {
          label.element.style.left = label.x + "px";
          label.element.style.top = label.y + "px";
          hops.push(label);
        }
      }
      const occupied = [];
      for (const label of projected.filter((l) => l.type === "axis")) {
        const outside =
          label.outside ||
          label.x < 0 ||
          label.x > w ||
          label.y < 0 ||
          label.y > h;
        const overlap = occupied.some(
          (o) => Math.abs(o.x - label.x) < 52 && Math.abs(o.y - label.y) < 15,
        );
        label.element.hidden = outside || overlap;
        if (!outside && !overlap) {
          label.element.style.left = label.x + "px";
          label.element.style.top = label.y + "px";
          occupied.push(label);
        }
      }
    }
  }

  return { MemoryScene };
})();
