/* Behavior checks use the real bundled Three.js renderer and browser DOM. */
globalThis.checkSceneBehavior = async () => {
  const { THREE } = KMP_THREE;
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const container = document.getElementById("scene-world"), canvas = document.getElementById("canvas");
  const selected = [], hovered = [];
  const scene = new KMP_APP.three.MemoryScene(container, canvas, ref => selected.push(ref), ref => hovered.push(ref));
  const a = { entry: { ref: "a", about: "alpha", kind: "decision" }, x: -180, y: 0, z: 10 };
  const b = { entry: { ref: "b", about: "alpha", kind: "observation" }, x: 180, y: -30, z: 10 };
  const c = { entry: { ref: "c", about: "beta", kind: "constraint" }, x: 0, y: 20, z: -120 };
  const edges = [{ source: "a", target: "b", rel: "follows", class: "procedural" }, { source: "b", target: "a", rel: "supersedes", class: "temporal" }];
  const layout = { planes: [{ about: "alpha", index: 0, y: 0, z: 0 }, { about: "beta", index: 1, y: 0, z: -130 }],
    nodes: [a, b, c], shownRelations: edges, neighbors: new Set(["b"]), times: [0, 1000, 2000], timeX: t => -410 + t / 2000 * 820 };
  const state = { mode: "3d", selected: "a", opacity: 30, relations: "all", scale: "elapsed", from: 0, to: 2000 };
  const render = () => scene.update(layout, state);
  const eventAt = ref => {
    const p = scene.nodeMeshes.get(ref).getWorldPosition(new THREE.Vector3()).project(scene.camera), rect = canvas.getBoundingClientRect();
    return { clientX: rect.left + (p.x + 1) * rect.width / 2, clientY: rect.top + (1 - p.y) * rect.height / 2 };
  };
  const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
  render(); const firstNodes = new Map(scene.nodeMeshes), firstPlanes = new Map(scene.planes.records);
  const paths = new Map(scene.relationLines.paths);
  for (const mode of ["flat", "3d"]) {
    state.mode = mode; render();
    for (const ref of ["a", "b"]) assert(scene.pick(eventAt(ref))?.userData.ref === ref, `pick ${ref} in ${mode}`);
    canvas.dispatchEvent(new PointerEvent("pointerdown", eventAt("a")));
    canvas.dispatchEvent(new PointerEvent("pointerup", eventAt("a")));
  }
  assert(selected.join() === "a,a", "selection callbacks preserve refs");
  for (let i = 0; i < 50; i++) canvas.dispatchEvent(new PointerEvent("pointermove", eventAt(i === 49 ? "b" : "a")));
  assert(hovered.length === 0, "hover must wait for the frame"); await frame();
  assert(hovered.length === 1 && hovered[0] === "b", "latest hover once per frame");
  canvas.dispatchEvent(new PointerEvent("pointermove", eventAt("a")));
  canvas.dispatchEvent(new PointerEvent("pointerleave")); await frame();
  assert(hovered.at(-1) === null && hovered.length === 2, "leave cancels pending hover");
  for (const [key, arrow] of scene.relationLines.arrows) {
    const curve = scene.relationLines.paths.get(key);
    const forward = key.startsWith("a ");
    const source = forward ? a : b, target = forward ? b : a;
    assert(curve.vertices[0] === source.x && curve.vertices[1] === source.y && curve.vertices[2] === source.z + 2, "line starts at source");
    assert(curve.vertices.at(-3) === target.x && curve.vertices.at(-2) === target.y && curve.vertices.at(-1) === target.z + 2, "line ends at target");
    assert(Math.sign(curve.tangent.x) === Math.sign(target.x - source.x), "arrow points from source to target");
    assert(arrow.position.distanceTo(curve.tip) < 1e-8, "arrow near destination");
    assert(new THREE.Vector3(0, 1, 0).applyQuaternion(arrow.quaternion).distanceTo(curve.tangent) < 1e-8, "arrow follows directed tangent");
  }
  state.opacity = 62; state.selected = "b"; state.relations = "selection"; a.dimmed = true;
  document.documentElement.dataset.theme = "light"; render();
  assert(scene.planes.records.get("alpha").panel.material.opacity === 0.62, "plane opacity");
  assert(scene.nodeMeshes.get("a").material.opacity === 0.18, "dimmed alpha");
  assert(scene.nodeMeshes.get("c").material.opacity === 0.85, "unlinked light fading");
  assert(scene.ring.userData.ref === "b" && scene.ring.visible, "selection marker updates");
  for (const [ref, mesh] of scene.nodeMeshes) assert(firstNodes.get(ref) === mesh, "retained node identity");
  for (const [about, plane] of scene.planes.records) assert(firstPlanes.get(about) === plane, "retained plane and label identity");
  for (const [key, curve] of paths) assert(scene.relationLines.paths.get(key) === curve, "style changes retain curves");
  state.selected = null; state.trace = { edgeKeys: new Set(["b supersedes a"]) }; render();
  assert(scene.relationLines.arrows.size === 1 && scene.relationLines.arrows.has("b supersedes a"), "trace direction independently selected");
  b.x = 220; render(); assert(scene.relationLines.paths.get("a follows b") !== paths.get("a follows b"), "moved endpoints replace curves");
  let removedMaterial = 0; scene.nodeMeshes.get("c").material.addEventListener("dispose", () => removedMaterial++);
  layout.nodes = [a, b]; layout.planes = layout.planes.slice(0, 1); layout.shownRelations = []; render();
  assert(removedMaterial === 1 && !scene.nodeMeshes.has("c"), "removed nodes dispose material");
  assert(scene.planes.records.size === 1 && !firstPlanes.get("beta").label.element.isConnected, "removed layer releases labels");
  assert(scene.relationLines.paths.size === 0 && scene.relationLines.batches.size === 0 && scene.relationLines.arrows.size === 0, "hidden relations release buffers");
  const resourceCycles = [];
  for (let i = 0; i < 8; i++) {
    layout.nodes = [a, b, c]; layout.shownRelations = edges; render();
    resourceCycles.push(scene.renderer.info.memory.geometries);
    scene.clear(); scene.draw();
  }
  assert(resourceCycles.every(n => n === resourceCycles[0]), "resource count must plateau across replacements");
  state.trace = null;
  for (const count of [1, 32, 1, 64, 2, 1]) {
    layout.shownRelations = Array.from({ length: count }, (_, i) => ({ source: "a", target: "b", rel: `synthetic-${i}`, class: "procedural" }));
    render();
    assert(scene.relationLines.batches.size === 1, "compatible edges share one buffer");
    const batch = [...scene.relationLines.batches.values()][0];
    assert(batch.line.geometry.drawRange.count === count * 80, "draw range follows live segments after grow/shrink");
    const capacity = batch.line.geometry.getAttribute("position").count;
    assert(capacity >= count * 80 && capacity <= count * 80 * 4, "buffer capacity stays bounded");
  }
  render(); const info = scene.renderer.info; scene.dispose(); scene.dispose();
  assert(info.memory.geometries === 0 && info.memory.textures === 0, "GPU resources released on disposal");
  assert(document.getElementById("scene-labels").childElementCount === 0, "DOM labels released");
  const oldHover = hovered.length;
  canvas.dispatchEvent(new PointerEvent("pointermove", { clientX: 100, clientY: 100 })); await frame();
  assert(hovered.length === oldHover, "disposed listeners stay silent");
  return { passed: true, resource_cycles: resourceCycles, final_geometries: info.memory.geometries };
};
