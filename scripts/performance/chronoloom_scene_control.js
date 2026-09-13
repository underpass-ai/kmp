/* Synthetic, deterministic application data with actual Three.js and WebGL. */
globalThis.sceneControl = async ({ entries: count, degree, samples }) => {
  const start = Date.parse("2026-09-01T00:00:00Z"), end = start + count * 60000;
  const refs = Array.from({ length: count }, (_, i) => `project:scene:entry:observation:n${i}`);
  const projection = {
    entries: refs.map((ref, i) => ({ ref_id: ref, kind: i % 3 ? "observation" : "decision",
      text: `Synthetic memory ${i}`, coordinates: [{ dimension: "task", scope_id: "task:scene",
        observed_at: new Date(start + i * 60000).toISOString() }] })),
    relations: refs.flatMap((ref, i) => Array.from({ length: degree }, (_, n) => ({
      from: ref, to: refs[(i + n + 1) % count], rel: "follows", class: "procedural",
      why: "Synthetic dependency", evidence: "Fixed control" }))),
  };
  const layers = [{ about: "project:scene", lod: "moment", projection }];
  const state = { mode: "3d", gap: 130, opacity: 30, clock: "observed", from: start, to: end,
    scale: "event_density", relations: "all", selected: null };
  let lens = KMP_LOOM.temporalLens({mode: state.scale, t0: start, t1: end,
    events: refs.map((_, i) => start + i * 60000)});
  const picked = [];
  const renderer = new KMP_APP.three.MemoryScene(document.getElementById("scene-world"),
    document.getElementById("canvas"), ref => picked.push(ref), () => {});
  const longTasks = [];
  const observer = new PerformanceObserver(list => longTasks.push(...list.getEntries().map(e => ({ start: e.startTime, duration: e.duration }))));
  observer.observe({ type: "longtask", buffered: true });
  const nextFrame = () => new Promise(resolve => requestAnimationFrame(resolve));
  const resources = () => ({ ...renderer.renderer.info.memory, calls: renderer.renderer.info.render.calls,
    heap: performance.memory?.usedJSHeapSize, objects: renderer.group.children.length });
  const identities = () => {
    const objects = new Set(), geometries = new Set(), materials = new Set();
    renderer.group.traverse(object => {
      objects.add(object.id);
      if (object.geometry) geometries.add(object.geometry.id);
      if (object.material) materials.add(object.material.id);
    });
    return { objects, geometries, materials };
  };
  const rows = [];
  const render = scenario => {
    performance.mark(`${scenario}-start`);
    const then = performance.now();
    const layout = KMP_APP.sceneModel.layout(layers, state, time => lens.toRatio(time));
    const mapped = performance.now();
    const previous = new Map(renderer.meshes.map(mesh => [mesh.userData.ref, mesh]));
    const before = identities();
    renderer.update(layout, state);
    const done = performance.now();
    const after = identities();
    const created = Object.fromEntries(Object.entries(after).map(([key, ids]) => [key + "_created", [...ids].filter(id => !before[key].has(id)).length]));
    performance.mark(`${scenario}-end`);
    performance.measure(scenario, `${scenario}-start`, `${scenario}-end`);
    return { scenario, ...created, layout_ms: mapped - then, update_ms: done - mapped, total_ms: done - then,
      reused_nodes: renderer.meshes.filter(mesh => previous.get(mesh.userData.ref) === mesh).length,
      nodes: renderer.meshes.length, ...resources() };
  };
  rows.push(render("initial"));
  await nextFrame(); await nextFrame();
  for (const scenario of ["opacity", "selection", "window", "mode", "relations"]) {
    for (let i = 0; i < samples; i++) {
      if (scenario === "opacity") state.opacity = 20 + i;
      if (scenario === "selection") state.selected = refs[i];
      if (scenario === "window") {
        state.from = start + i * 1000;
        lens = KMP_LOOM.temporalLens({ mode: state.scale, t0: state.from, t1: end,
          events: refs.map((_, n) => start + n * 60000) });
      }
      if (scenario === "mode") state.mode = i % 2 ? "3d" : "flat";
      if (scenario === "relations") state.relations = i % 2 ? "all" : "selection";
      const beforeFrame = await nextFrame();
      rows.push(render(scenario));
      const afterFrame = await nextFrame();
      rows.at(-1).frame_interval_ms = afterFrame - beforeFrame;
    }
  }
  // Verify the current mesh-to-reference mapping through real ray casting.
  const mesh = renderer.meshes[Math.floor(renderer.meshes.length / 2)];
  const projected = mesh.getWorldPosition(new KMP_THREE.THREE.Vector3()).project(renderer.camera);
  const rect = renderer.renderer.domElement.getBoundingClientRect();
  const point = { clientX: rect.left + (projected.x + 1) * rect.width / 2,
    clientY: rect.top + (1 - projected.y) * rect.height / 2 };
  const hit = renderer.pick(point);
  if (!hit || !refs.includes(hit.userData.ref)) throw new Error("ray picking lost its reference");
  const beforeClear = resources();
  renderer.clear(); renderer.draw();
  const afterClear = resources();
  rows.push(render("repopulate"));
  await nextFrame(); await nextFrame();
  const finalResources = resources();
  const supportsDispose = typeof renderer.dispose === "function";
  const gl = renderer.renderer.getContext(), extension = gl.getExtension("WEBGL_debug_renderer_info");
  const gpu = extension ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  globalThis.disposeSceneControl = () => {
    if (!supportsDispose) return null;
    const info = renderer.renderer.info;
    renderer.dispose();
    return { ...info.memory, labels: document.getElementById("scene-labels").childElementCount, meshes: renderer.meshes.length };
  };
  observer.disconnect();
  return { count, degree, samples, rows, long_tasks: longTasks, pick: hit.userData.ref,
    before_clear: beforeClear, after_clear: afterClear, final_resources: finalResources,
    supports_dispose: supportsDispose,
    gpu };
};
