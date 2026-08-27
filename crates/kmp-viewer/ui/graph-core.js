/* KMP Memory Viewer — graph core.
   Pure logic only: no DOM, no PIXI, no fetch. Everything here takes plain
   data and returns plain data, so the algorithmic half of the viewer can be
   reasoned about — and one day harnessed — without a browser in the room.
   Loaded before viewer.js; exposes a single global, KMP_CORE. */
"use strict";

const KMP_CORE = (() => {
  /* ---------------- simulation constants ----------------
     One object on purpose: tuning is a single place, not a scavenger hunt.
     The scheme is d3-force's — forces accumulate into velocities scaled by
     alpha, velocity decays each tick, positions integrate — because that
     scheme is known to settle where naive springs are known to explode. */
  const SIM = {
    ALPHA_START: 1.0,
    ALPHA_MIN: 0.001,
    ALPHA_DECAY: 0.0228, // ~300 ticks from 1.0 to rest
    VELOCITY_DECAY: 0.4, // velocity keeps 60% per tick
    VMAX: 40, // hard cap, world units per tick — nothing outruns this
    CHARGE: -1100, // many-body repulsion strength
    THETA: 0.85, // Barnes-Hut acceptance: cell width / distance
    DIST_MIN2: 16, // repulsion saturates below 4 units
    DIST_MAX2: 2.25e6, // and vanishes beyond 1500
    LINK_REST: 90, // spring rest length
    CONTAIN: 0.02, // weak pull to origin — keeps stray components home
    COLLIDE_PAD: 3, // extra radius when resolving overlaps
    COLLIDE_FROM_ALPHA: 0.5, // collide only once the layout has calmed
    REHEAT_LOAD: 1.0,
    REHEAT_EXPAND: 0.5,
    REHEAT_DRAG: 0.3,
  };

  /* ---------------- deterministic randomness ----------------
     The layout must be a picture of the memory, not of the dice: the same
     store renders the same drawing on every reload. Node ids seed a small
     PRNG; nothing here touches crypto. */

  function fnv1a(text) {
    let hash = 0x811c9dc5;
    for (let i = 0; i < text.length; i += 1) {
      hash ^= text.charCodeAt(i);
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    return hash >>> 0;
  }

  function mulberry32(seed) {
    let state = seed >>> 0;
    return () => {
      state = (state + 0x6d2b79f5) >>> 0;
      let t = state;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 0x100000000;
    };
  }

  /* Golden-angle spiral: the i-th new node lands on a widening spiral around
     its anchor, jittered a touch so coincident anchors do not stack. Dense,
     round, and the same every time. */
  function spiralPoint(index, id, anchorX, anchorY) {
    const rng = mulberry32(fnv1a(id));
    const radius = 22 * Math.sqrt(index + 1);
    const angle = (index + 1) * 2.39996322972865332;
    return {
      x: anchorX + Math.cos(angle) * radius + (rng() - 0.5) * 8,
      y: anchorY + Math.sin(angle) * radius + (rng() - 0.5) * 8,
    };
  }

  /* ---------------- quadtree (Barnes-Hut) ----------------
     Flat-object quadtree rebuilt per tick. Internal cells carry center of
     mass; leaves carry one node (coincident nodes chain in `next`). */

  function buildQuadtree(nodes) {
    if (!nodes.length) return null;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const node of nodes) {
      if (node.x < minX) minX = node.x;
      if (node.y < minY) minY = node.y;
      if (node.x > maxX) maxX = node.x;
      if (node.y > maxY) maxY = node.y;
    }
    const side = Math.max(maxX - minX, maxY - minY, 1);
    const root = { x0: minX, y0: minY, side, children: null, node: null, mass: 0, cx: 0, cy: 0 };
    for (const node of nodes) insertQuad(root, node);
    aggregateQuad(root);
    return root;
  }

  function insertQuad(cell, node) {
    for (;;) {
      if (cell.children) {
        cell = childFor(cell, node);
        continue;
      }
      if (!cell.node) {
        cell.node = node;
        return;
      }
      const resident = cell.node;
      // Coincident points cannot be split apart; chain them.
      if (resident.x === node.x && resident.y === node.y) {
        node._next = resident._next;
        resident._next = node;
        return;
      }
      cell.node = null;
      cell.children = [null, null, null, null];
      splitInsert(cell, resident);
      cell = childFor(cell, node);
    }
  }

  function splitInsert(cell, node) {
    childFor(cell, node).node = node;
  }

  function childFor(cell, node) {
    const half = cell.side / 2;
    const right = node.x >= cell.x0 + half ? 1 : 0;
    const down = node.y >= cell.y0 + half ? 1 : 0;
    const index = down * 2 + right;
    if (!cell.children[index]) {
      cell.children[index] = {
        x0: cell.x0 + right * half,
        y0: cell.y0 + down * half,
        side: half,
        children: null,
        node: null,
        mass: 0,
        cx: 0,
        cy: 0,
      };
    }
    return cell.children[index];
  }

  function aggregateQuad(cell) {
    if (!cell) return;
    if (cell.children) {
      let mass = 0;
      let cx = 0;
      let cy = 0;
      for (const child of cell.children) {
        if (!child) continue;
        aggregateQuad(child);
        mass += child.mass;
        cx += child.cx * child.mass;
        cy += child.cy * child.mass;
      }
      cell.mass = mass;
      cell.cx = mass ? cx / mass : cell.x0;
      cell.cy = mass ? cy / mass : cell.y0;
      return;
    }
    let mass = 0;
    let cx = 0;
    let cy = 0;
    for (let node = cell.node; node; node = node._next) {
      const m = node.mass || 1;
      mass += m;
      cx += node.x * m;
      cy += node.y * m;
    }
    cell.mass = mass;
    cell.cx = mass ? cx / mass : cell.x0;
    cell.cy = mass ? cy / mass : cell.y0;
  }

  function nudge(node, fromX, fromY, mass, alpha) {
    let dx = node.x - fromX;
    let dy = node.y - fromY;
    let d2 = dx * dx + dy * dy;
    if (d2 >= SIM.DIST_MAX2) return;
    if (d2 === 0) {
      // Coincident: push along a fixed direction — stable and id-free.
      dx = 0.5;
      dy = 0.5;
      d2 = 0.5;
    }
    if (d2 < SIM.DIST_MIN2) d2 = SIM.DIST_MIN2;
    const strength = (SIM.CHARGE * alpha * mass) / d2;
    const dist = Math.sqrt(d2);
    node.vx -= (dx / dist) * strength;
    node.vy -= (dy / dist) * strength;
  }

  function applyRepulsion(tree, node, alpha) {
    if (!tree || !tree.mass) return;
    const stack = [tree];
    while (stack.length) {
      const cell = stack.pop();
      if (!cell.mass) continue;
      if (cell.children) {
        const dx = node.x - cell.cx;
        const dy = node.y - cell.cy;
        const d2 = dx * dx + dy * dy;
        if (cell.side * cell.side < SIM.THETA * SIM.THETA * d2) {
          nudge(node, cell.cx, cell.cy, cell.mass, alpha);
        } else {
          for (const child of cell.children) {
            if (child && child.mass) stack.push(child);
          }
        }
        continue;
      }
      for (let resident = cell.node; resident; resident = resident._next) {
        if (resident === node) continue;
        nudge(node, resident.x, resident.y, resident.mass || 1, alpha);
      }
    }
  }

  /* ---------------- forces ---------------- */

  /* d3-force-link's exact scheme: strength normalized by the smaller degree
     so a hub touching five hundred edges is not five hundred times stiffer,
     and the pull is split by degree bias so the lighter end does the moving. */
  function applyLinks(edges, byId, alpha) {
    for (const edge of edges) {
      const s = byId(edge.source);
      const t = byId(edge.target);
      if (!s || !t || s === t) continue;
      let dx = t.x + t.vx - s.x - s.vx;
      let dy = t.y + t.vy - s.y - s.vy;
      let dist = Math.hypot(dx, dy);
      if (dist < 1) {
        dx = 0.5;
        dy = 0.5;
        dist = Math.SQRT1_2;
      }
      const degS = Math.max(1, s.degree || 1);
      const degT = Math.max(1, t.degree || 1);
      const strength = Math.min(0.7, Math.max(0.02, 0.7 / Math.min(degS, degT)));
      const pull = ((dist - SIM.LINK_REST) / dist) * alpha * strength;
      const bias = degS / (degS + degT);
      t.vx -= dx * pull * bias;
      t.vy -= dy * pull * bias;
      s.vx += dx * pull * (1 - bias);
      s.vy += dy * pull * (1 - bias);
    }
  }

  /* d3's forceCenter: shift every position so the centroid sits at the
     origin. A shift, not a force — it cannot add energy, so it cannot make
     anything oscillate. Skipped while a node is pinned under the pointer. */
  function applyCenter(nodes) {
    let cx = 0;
    let cy = 0;
    for (const node of nodes) {
      cx += node.x;
      cy += node.y;
    }
    cx /= nodes.length;
    cy /= nodes.length;
    for (const node of nodes) {
      node.x -= cx;
      node.y -= cy;
    }
  }

  /* Grid-hash collision pass: push overlapping pairs apart. Only runs in the
     cooling half of the layout — while hot, overlap is information the other
     forces still need. */
  function applyCollide(nodes, radiusOf) {
    let maxR = 0;
    for (const node of nodes) {
      const r = radiusOf(node) + SIM.COLLIDE_PAD;
      node._cr = r;
      if (r > maxR) maxR = r;
    }
    const cellSize = maxR * 2 || 1;
    const grid = new Map();
    const keyOf = (x, y) => `${Math.floor(x / cellSize)},${Math.floor(y / cellSize)}`;
    for (const node of nodes) {
      const key = keyOf(node.x, node.y);
      let bucket = grid.get(key);
      if (!bucket) grid.set(key, (bucket = []));
      bucket.push(node);
    }
    for (const node of nodes) {
      const cellX = Math.floor(node.x / cellSize);
      const cellY = Math.floor(node.y / cellSize);
      for (let gx = cellX - 1; gx <= cellX + 1; gx += 1) {
        for (let gy = cellY - 1; gy <= cellY + 1; gy += 1) {
          const bucket = grid.get(`${gx},${gy}`);
          if (!bucket) continue;
          for (const other of bucket) {
            if (other === node || other._collided === node) continue;
            const minDist = node._cr + other._cr;
            let dx = other.x - node.x;
            let dy = other.y - node.y;
            let d2 = dx * dx + dy * dy;
            if (d2 >= minDist * minDist) continue;
            if (d2 === 0) {
              dx = 0.5;
              dy = 0;
              d2 = 0.25;
            }
            const dist = Math.sqrt(d2);
            const push = ((minDist - dist) / dist) * 0.5;
            node.x -= dx * push;
            node.y -= dy * push;
            other.x += dx * push;
            other.y += dy * push;
            node._collided = other;
          }
        }
      }
    }
    for (const node of nodes) {
      node._collided = null;
      node._cr = 0;
    }
  }

  /* ---------------- one step ----------------
     Mutates x/y/vx/vy in place. `opts.pinned` is the node under the pointer
     (its position is the pointer's, not the simulation's); `opts.radiusOf`
     maps a node to its drawn radius for the collide pass. */
  function simStep(nodes, edges, byId, alpha, opts = {}) {
    if (!nodes.length) return;
    const pinned = opts.pinned || null;
    const radiusOf = opts.radiusOf || (() => 8);

    applyLinks(edges, byId, alpha);
    const tree = buildQuadtree(nodes);
    for (const node of nodes) {
      applyRepulsion(tree, node, alpha);
      // Weak containment keeps disconnected components from drifting away.
      node.vx -= node.x * SIM.CONTAIN * alpha;
      node.vy -= node.y * SIM.CONTAIN * alpha;
    }
    for (const node of nodes) node._next = null;

    const keep = 1 - SIM.VELOCITY_DECAY;
    for (const node of nodes) {
      node.vx *= keep;
      node.vy *= keep;
      const speed = Math.hypot(node.vx, node.vy);
      if (speed > SIM.VMAX) {
        node.vx = (node.vx / speed) * SIM.VMAX;
        node.vy = (node.vy / speed) * SIM.VMAX;
      }
      if (node !== pinned) {
        node.x += node.vx;
        node.y += node.vy;
      }
    }

    if (!pinned) applyCenter(nodes);
    if (alpha < SIM.COLLIDE_FROM_ALPHA) applyCollide(nodes, radiusOf);
  }

  /* ---------------- framing ---------------- */

  function computeBounds(nodes, include) {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    let any = false;
    for (const node of nodes) {
      if (include && !include(node)) continue;
      any = true;
      if (node.x < minX) minX = node.x;
      if (node.y < minY) minY = node.y;
      if (node.x > maxX) maxX = node.x;
      if (node.y > maxY) maxY = node.y;
    }
    return any ? { minX, minY, maxX, maxY } : null;
  }

  /* The camera {x, y, k} that shows `bounds` inside width×height with some
     breathing room. k is clamped: a two-node graph should not fill the wall,
     an empty one should not zoom to infinity. */
  function fitTransform(bounds, width, height, pad = 64) {
    if (!bounds) return null;
    const spanX = Math.max(bounds.maxX - bounds.minX, 1);
    const spanY = Math.max(bounds.maxY - bounds.minY, 1);
    const k = Math.min(
      2.5,
      Math.max(0.05, Math.min((width - pad * 2) / spanX, (height - pad * 2) / spanY))
    );
    return {
      x: (bounds.minX + bounds.maxX) / 2,
      y: (bounds.minY + bounds.maxY) / 2,
      k,
    };
  }

  return {
    SIM,
    fnv1a,
    mulberry32,
    spiralPoint,
    simStep,
    computeBounds,
    fitTransform,
  };
})();
