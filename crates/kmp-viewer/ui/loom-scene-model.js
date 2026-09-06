/* Pure projection-to-scene mapping. About ownership is independent of labels.
   Coarse records keep their aggregate identity; only stored relations are edges. */
"use strict";
KMP_APP.sceneModel = (() => {
  function layout(layers, state, timeRatio) {
    const planes = layers.map((layer, index) => ({
      about: layer.about,
      index,
      y: state.mode === "flat" ? ((layers.length - 1) / 2 - index) * 310 : 0,
      z:
        state.mode === "3d" ? ((layers.length - 1) / 2 - index) * state.gap : 0,
      caption:
        layer.error ||
        (layer.lod === "moment"
          ? `${(layer.projection.entries || []).length} memories${layer.projection.truncated ? " · partial" : ""}`
          : `${layer.lod} · label aggregates${layer.projection.truncated ? " · partial" : ""}`),
    }));
    const nodes = [],
      seen = new Set(),
      relations = [],
      times = [];
    for (const plane of planes) {
      const layer = layers[plane.index],
        projection = layer.projection;
      const ends = Array(7).fill(-Infinity);
      let entries = (projection.entries || [])
        .map(KMP_LOOM.entryModel)
        .map((entry) => ({ ...entry, about: plane.about }));
      if (layer.lod !== "moment") {
        const source = (projection.clusters || []).length
          ? projection.clusters
          : projection.bins || [];
        entries = source.map((item, index) => ({
          about: plane.about,
          ref: `aggregate:${plane.index}:${index}`,
          aggregate: true,
          kind: "aggregate",
          from: Date.parse(item.from),
          to: Date.parse(item.to),
          text: `${item.dimension}=${item.scope_id || "*"} · ${item.total} label placements`,
        }));
      }
      entries = entries
        .map((entry) => ({
          entry,
          time: entry.aggregate
            ? entry.from
            : KMP_LOOM.strictMs(entry, state.clock),
        }))
        .filter(
          ({ time }) =>
            time !== null &&
            Number.isFinite(time) &&
            time >= state.from &&
            time < state.to,
        )
        .sort(
          (a, b) => a.time - b.time || a.entry.ref.localeCompare(b.entry.ref),
        );
      for (const { entry, time } of entries) {
        if (seen.has(entry.ref)) continue;
        seen.add(entry.ref);
        times.push(time);
        const x = -410 + 820 * timeRatio(time);
        let track = ends.findIndex((end) => x - end > 29);
        if (track < 0) track = ends.indexOf(Math.min(...ends));
        ends[track] = x;
        const dimmed =
          state.dimmedKinds?.has(entry.kind) ||
          (state.searchHits?.size && !state.searchHits.has(entry.ref)) ||
          (state.trace && !state.trace.refs.has(entry.ref)) ||
          (!entry.aggregate &&
            state.hiddenLanes?.size &&
            entry.coords.every((coord) =>
              state.hiddenLanes.has(coord.dimension),
            ));
        nodes.push({
          entry,
          x,
          y: plane.y + 82 - track * 28,
          z: plane.z + 3,
          dimmed: Boolean(dimmed),
        });
      }
      relations.push(
        ...(projection.relations || []).map((edge) => ({
          ...edge,
          source: edge.from,
          target: edge.to,
        })),
      );
    }
    const edges = new Map();
    for (const edge of relations) {
      if (
        seen.has(edge.source) &&
        seen.has(edge.target) &&
        (!state.relationClasses || state.relationClasses.includes(edge.class))
      )
        edges.set(`${edge.source} ${edge.rel} ${edge.target}`, edge);
    }
    const stored = [...edges.values()];
    const selected = stored.filter(
      (edge) =>
        edge.source === state.selected || edge.target === state.selected,
    );
    const traced = state.trace
      ? stored.filter((edge) =>
          state.trace.edgeKeys.has(`${edge.source} ${edge.rel} ${edge.target}`),
        )
      : null;
    return {
      planes,
      nodes,
      relations: stored,
      shownRelations:
        traced ||
        (state.relations === "none"
          ? []
          : state.relations === "all"
            ? stored
            : selected),
      neighbors: new Set(
        (traced || selected).flatMap((edge) => [edge.source, edge.target]),
      ),
      times: [...new Set(times)].sort((a, b) => a - b),
      timeX: (time) => -410 + 820 * timeRatio(time),
    };
  }
  return { layout };
})();
