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
  const aboutColors = ["#af95f1", "#64bba1", "#76a4d1", "#c99abb", "#d3b773"];
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
      this.controls.addEventListener("change", () => this.draw());
      this.scene.add(new THREE.AmbientLight(0xffffff, 2));
      const light = new THREE.DirectionalLight(0xffffff, 3);
      light.position.set(200, 600, 900);
      this.scene.add(light);
      this.group = new THREE.Group();
      this.scene.add(this.group);
      this.raycaster = new THREE.Raycaster();
      this.pointer = new THREE.Vector2();
      this.meshes = [];
      this.labels = [];
      this.geometry = {
        decision: new THREE.OctahedronGeometry(5.5),
        constraint: new THREE.BoxGeometry(8, 8, 8),
        normal: new THREE.SphereGeometry(4.2, 14, 10),
      };
      this.onPointerDown = (e) => {
        this.down = { x: e.clientX, y: e.clientY };
      };
      canvas.addEventListener("pointerdown", this.onPointerDown);
      canvas.addEventListener("pointerup", (e) => {
        if (
          this.down &&
          Math.hypot(e.clientX - this.down.x, e.clientY - this.down.y) < 5
        ) {
          const hit = this.pick(e);
          if (hit) this.onSelect(hit.userData.ref);
        }
        this.down = null;
      });
      canvas.addEventListener("pointermove", (e) => {
        if (e.buttons) return;
        const hit = this.pick(e);
        canvas.style.cursor = hit ? "pointer" : "grab";
        this.onHover(hit?.userData.ref, e);
      });
      canvas.addEventListener("pointerleave", () => this.onHover(null));
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
    line(points, color, opacity = 1) {
      const geometry = new THREE.BufferGeometry().setFromPoints(
        points.map((p) => new THREE.Vector3(...p)),
      );
      const line = new THREE.Line(
        geometry,
        new THREE.LineBasicMaterial({
          color,
          transparent: true,
          opacity,
          depthWrite: false,
        }),
      );
      this.group.add(line);
      return line;
    }
    clear() {
      const shared = new Set(Object.values(this.geometry));
      this.group.traverse((o) => {
        if (o.geometry && !shared.has(o.geometry)) o.geometry.dispose();
        if (o.material) {
          const mats = Array.isArray(o.material) ? o.material : [o.material];
          mats.forEach((m) => {
            m.map?.dispose();
            m.dispose();
          });
        }
      });
      this.group.clear();
      this.meshes = [];
      this.labels = [];
      document.getElementById("scene-labels").replaceChildren();
    }
    update(layout, state) {
      const modeChanged = this.mode !== state.mode;
      const layersChanged = this.layout?.planes.length !== layout.planes.length;
      this.mode = state.mode;
      this.layout = layout;
      this.state = state;
      this.clear();
      const style = getComputedStyle(document.documentElement),
        bg = style.getPropertyValue("--canvas").trim(),
        light = document.documentElement.dataset.theme === "light";
      this.renderer.setClearColor(bg);
      this.scene.background = new THREE.Color(bg);
      const textContainer = document.getElementById("scene-labels");
      const leaders = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "svg",
      );
      leaders.classList.add("plane-leaders");
      textContainer.append(leaders);
      layout.planes.forEach((plane) => {
        const color = light
          ? ["#7954b4", "#287d65", "#416f9d", "#97537f", "#896922"][
              plane.index % 5
            ]
          : aboutColors[plane.index % aboutColors.length];
        const panel = new THREE.Mesh(
          new THREE.PlaneGeometry(940, 280),
          new THREE.MeshBasicMaterial({
            color: light ? "#c0d6c8" : "#314d40",
            side: THREE.DoubleSide,
            transparent: true,
            opacity: state.opacity / 100,
            depthWrite: false,
          }),
        );
        panel.position.set(0, plane.y, plane.z);
        this.group.add(panel);
        this.line(
          [
            [-470, plane.y + 140, plane.z],
            [470, plane.y + 140, plane.z],
            [470, plane.y - 140, plane.z],
            [-470, plane.y - 140, plane.z],
            [-470, plane.y + 140, plane.z],
          ],
          color,
          0.38,
        );
        this.line(
          [
            [-470, plane.y + 140, plane.z],
            [-470, plane.y - 140, plane.z],
          ],
          color,
          0.9,
        );
        for (let i = 0; i <= 8; i++) {
          const x = -410 + (820 * i) / 8;
          this.line(
            [
              [x, plane.y + 102, plane.z + 0.1],
              [x, plane.y - 108, plane.z + 0.1],
            ],
            light ? "#60816d" : "#6f9880",
            0.14,
          );
        }
        this.line(
          [
            [-410, plane.y - 116, plane.z + 0.5],
            [410, plane.y - 116, plane.z + 0.5],
          ],
          color,
          0.5,
        );
        const label = document.createElement("div");
        label.className = "plane-label";
        label.style.setProperty("--plane-color", color);
        const title = document.createElement("span");
        title.textContent = plane.about;
        const subtitle = document.createElement("small");
        subtitle.textContent =
          plane.caption ||
          layout.nodes.filter((n) => n.entry.about === plane.about).length +
            " memories";
        label.append(title, subtitle);
        textContainer.append(label);
        const leader = document.createElementNS(
          "http://www.w3.org/2000/svg",
          "line",
        );
        leader.setAttribute("stroke", color);
        leaders.append(leader);
        this.labels.push({
          element: label,
          leader,
          point: new THREE.Vector3(-470, plane.y + 140, plane.z),
          type: "plane",
        });
        if (plane.index === 0 || state.mode === "flat") {
          const count = 4;
          for (let i = 0; i < count; i++) {
            const rank = layout.times.length
              ? Math.round(((layout.times.length - 1) * i) / (count - 1))
              : 0;
            const time =
              state.scale === "elapsed"
                ? state.from + ((state.to - state.from) * i) / (count - 1)
                : layout.times[rank];
            if (time === undefined) continue;
            const x =
              state.scale === "elapsed"
                ? -410 + (820 * i) / (count - 1)
                : layout.timeX(time);
            const tick = document.createElement("span");
            tick.className = "axis-label";
            tick.textContent = new Intl.DateTimeFormat("en-GB", {
              day: "2-digit",
              month: "short",
              timeZone: "UTC",
            }).format(time);
            textContainer.append(tick);
            this.labels.push({
              element: tick,
              point: new THREE.Vector3(x, plane.y - 118, plane.z),
              type: "axis",
            });
          }
        }
      });
      const nodeByRef = new Map(layout.nodes.map((n) => [n.entry.ref, n]));
      for (const node of layout.nodes) {
        const selected = node.entry.ref === state.selected,
          linked = layout.neighbors.has(node.entry.ref),
          color =
            (light ? lightKindColors : kindColors)[node.entry.kind] ||
            kindColors.observation;
        const faded =
          node.dimmed ||
          (state.relations === "selection" &&
            layout.neighbors.size > 0 &&
            !selected &&
            !linked);
        const material = new THREE.MeshStandardMaterial({
          color,
          roughness: 0.5,
          metalness: 0.12,
          emissive: color,
          emissiveIntensity: selected ? 0.7 : 0.1,
          transparent: faded,
          opacity: node.dimmed ? 0.18 : faded ? (light ? 0.85 : 0.64) : 1,
        });
        const mesh = new THREE.Mesh(
          node.entry.aggregate
            ? this.geometry.constraint
            : this.geometry[node.entry.kind] || this.geometry.normal,
          material,
        );
        mesh.position.set(node.x, node.y, node.z);
        mesh.userData.ref = node.entry.ref;
        if (node.entry.aggregate) mesh.scale.set(2.4, 1.2, 0.3);
        if (selected) mesh.scale.setScalar(1.5);
        mesh.userData.baseScale = mesh.scale.clone();
        this.group.add(mesh);
        this.meshes.push(mesh);
        if (selected) {
          const ring = new THREE.Mesh(
            new THREE.RingGeometry(9, 10, 36),
            new THREE.MeshBasicMaterial({
              color,
              side: THREE.DoubleSide,
              transparent: true,
              opacity: 0.9,
            }),
          );
          ring.position.set(node.x, node.y, node.z + 0.3);
          ring.userData.marker = true;
          ring.userData.ref = node.entry.ref;
          this.group.add(ring);
        }
      }
      for (const edge of layout.shownRelations) {
        const a = nodeByRef.get(edge.source),
          b = nodeByRef.get(edge.target);
        if (!a || !b) continue;
        const selected =
          edge.source === state.selected ||
          edge.target === state.selected ||
          state.trace?.edgeKeys.has(
            `${edge.source} ${edge.rel} ${edge.target}`,
          );
        const color =
          edge.rel === "supersedes"
            ? light
              ? "#8e457b"
              : "#d69dc9"
            : KMP_APP.theme.classColor(edge.class);
        const bend = Math.min(85, Math.max(24, Math.abs(b.x - a.x) * 0.12));
        const start = new THREE.Vector3(a.x, a.y, a.z + 2),
          end = new THREE.Vector3(b.x, b.y, b.z + 2);
        const curve = new THREE.CubicBezierCurve3(
          start,
          new THREE.Vector3(a.x + (b.x - a.x) * 0.25, a.y + bend, a.z + 2),
          new THREE.Vector3(a.x + (b.x - a.x) * 0.75, b.y + bend, b.z + 2),
          end,
        );
        this.line(
          curve.getPoints(40).map((v) => v.toArray()),
          color,
          selected ? 0.85 : 0.18,
        );
        if (selected) {
          const tip = curve.getPoint(0.96),
            tangent = curve.getTangent(0.96).normalize(),
            arrow = new THREE.Mesh(
              new THREE.ConeGeometry(2.3, 7, 8),
              new THREE.MeshBasicMaterial({ color }),
            );
          arrow.position.copy(tip);
          arrow.quaternion.setFromUnitVectors(
            new THREE.Vector3(0, 1, 0),
            tangent,
          );
          this.group.add(arrow);
        }
      }
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
                mark.getWorldPosition(new THREE.Vector3()),
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
      const labelWidth = Math.max(
        0,
        ...planes.map((l) => l.element.offsetWidth),
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
        const height = label.element.offsetHeight || 38,
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
          label.leader.setAttribute("x1", left + label.element.offsetWidth + 4);
          label.leader.setAttribute("y1", y);
          label.leader.setAttribute("x2", label.x);
          label.leader.setAttribute("y2", label.y);
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
