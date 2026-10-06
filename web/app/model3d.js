// The model, shaded: a WebGL view (glview.js) of its parts, members and solids alike, each in
// the colour of what it is made of, hiding what is behind it. It needs no scan and no service:
// it is drawn from the mesh the engine gives the page (`/api/mesh`).

import { GLView, vec } from "./glview.js";
import * as objects from "./objects.js";

/** The colour of a material, by its name (a layer of a build-up, a solid's material). */
export function materialColour(name) {
  const n = String(name).toLowerCase();
  if (/standing|steel|metal/.test(n)) return "#9fb3c6";
  if (/tpo/.test(n)) return "#efefe9";
  if (/underlayment|polystick|felt|membrane/.test(n)) return "#50555e";
  if (/plywood|osb|wsp/.test(n)) return "#dfc088";
  if (/hardwood|flooring/.test(n)) return "#a8713f";
  if (/board/.test(n)) return "#b98a52";
  if (/shingle/.test(n)) return "#707070";
  if (/tile/.test(n)) return "#b5654a";
  if (/gypsum/.test(n)) return "#f3f1ea";
  if (/concrete/.test(n)) return "#b9b9b4";
  return "#c9c9c0";
}
const TIMBER = "#d8b683", PICKED = "#ff3cdc", DEFLECTED = [214, 40, 40, 255];
/** The colour of a member that is not lumber, by what it is made of. */
const STOCK = { steel: "#8f9aa6", aluminum: "#c9d0d6", cfrp: "#3a3d42", laminate: "#3a3d42", titanium: "#a9a59c" };
const rgb = (hex) => { const v = parseInt(hex.slice(1), 16); return [v >> 16, (v >> 8) & 255, v & 255]; };

/** A face's normal (Newell's: any polygon, any way round). */
function normal(f) {
  const n = [0, 0, 0];
  for (let i = 0; i < f.length; i++) {
    const a = f[i], b = f[(i + 1) % f.length];
    n[0] += (a[1] - b[1]) * (a[2] + b[2]); n[1] += (a[2] - b[2]) * (a[0] + b[0]); n[2] += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return vec.unit(n);
}

/** A face as triangles (ears cut off it: it may not be convex), as triples of its points. */
function triangles(f, n) {
  if (f.length < 3) return [];
  if (f.length === 3) return [f];
  // In its own plane.
  const u = vec.unit(Math.abs(n[2]) < 0.9 ? vec.cross([0, 0, 1], n) : vec.cross([1, 0, 0], n)), v = vec.cross(n, u);
  const p = f.map((q) => [vec.dot(q, u), vec.dot(q, v)]);
  const turn = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
  const idx = f.map((_, i) => i), out = [];
  let guard = f.length * f.length;
  while (idx.length > 3 && guard-- > 0) {
    let cut = false;
    for (let k = 0; k < idx.length; k++) {
      const [i, j, l] = [idx[(k + idx.length - 1) % idx.length], idx[k], idx[(k + 1) % idx.length]];
      if (turn(p[i], p[j], p[l]) <= 1e-12) continue;
      // (An ear: no other corner is in it.)
      if (idx.some((m) => m !== i && m !== j && m !== l && turn(p[i], p[j], p[m]) > 1e-12 && turn(p[j], p[l], p[m]) > 1e-12 && turn(p[l], p[i], p[m]) > 1e-12)) continue;
      out.push([f[i], f[j], f[l]]);
      idx.splice(k, 1);
      cut = true;
      break;
    }
    // (Nothing to cut: points in a line. What is left is fanned.)
    if (!cut) break;
  }
  for (let k = 1; k + 1 < idx.length; k++) out.push([f[idx[0]], f[idx[k]], f[idx[k + 1]]]);
  return out;
}

/**
 * Adds the pane ("3D, shaded"), and lists the model's parts in the tree of objects. `app`:
 * { registerPane, api, version (of the model as built), inspect (a member, by its path), shown
 * (whether a pane is), showBrowser, parked (where panes not shown are kept), scan (whether a scan
 * workspace is here: it then says what a member chosen is), deflects (whether the model's analysis
 * gives displacements), combination (the load combination chosen) }. Returns { load, sync }: `sync` when
 * the model has changed (the tree), `load` when the pane is shown too.
 */
export function initModel3d(app) {
  const pane = document.createElement("div");
  pane.id = "shadedPane";
  pane.className = "glpane";
  pane.style.background = "#f7f7f4";
  const host = document.createElement("div");
  host.className = "glhost";
  const bar = document.createElement("div");
  bar.style.cssText = "position:absolute;left:8px;top:8px;display:flex;gap:10px;align-items:center;font:12px system-ui;background:rgba(255,255,255,.85);border:1px solid #ccc;border-radius:6px;padding:4px 8px";
  bar.innerHTML = `<button data-a="fit">fit</button><label><input type="checkbox" data-a="edges" checked> edges</label><label><input type="checkbox" data-a="ortho"> parallel</label><span data-a="deflect" hidden style="display:inline-flex;gap:6px;align-items:center"><label title="The shape under the load combination chosen in Checks, exaggerated"><input type="checkbox" data-a="deflectOn" checked> deflected</label><input type="range" data-a="deflectScale" min="0" max="100" value="50" style="width:90px"><span data-a="deflectInfo" style="color:#666"></span></span><span data-a="what" style="color:#444;max-width:46ch;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"></span>`;
  const hint = document.createElement("div");
  hint.style.cssText = "position:absolute;left:8px;bottom:6px;font:11px system-ui;color:#777;pointer-events:none";
  hint.textContent = "drag to orbit · right-drag to pan · scroll to zoom · click a part";
  pane.append(host, bar, hint);
  app.parked.appendChild(pane);

  let view = null, mesh = null, version = null, name = null, picked = null, tris = null, owner = null, edges = true;
  // The deflected shape of a frame analysis (the engine's, for the combination chosen), and for which.
  let deflected = null, deflectedFor = null;
  const part = (a) => bar.querySelector(`[data-a="${a}"]`);
  const what = bar.querySelector('[data-a="what"]');

  function build() {
    const light = vec.unit([0.35, -0.55, 0.76]);
    const pos = [], col = [], lpos = [], lcol = [];
    owner = [];
    mesh.members.forEach((m, mi) => {
      // (What the tree of objects has hidden is not drawn, nor picked.)
      if (!objects.memberVisible(m.path)) return;
      const base = rgb(mi === picked ? PICKED : m.material ? materialColour(m.material) : STOCK[m.made_of] || TIMBER);
      // (A curved side is one surface: the edges between its facets are not edges of the part.)
      const facets = new Map(), key = (a, b) => [a, b].map((q) => q.join(",")).sort().join("|");
      if (m.smooth && edges) for (const f of m.faces) { const n = normal(f); for (let i = 0; i < f.length; i++) { const k = key(f[i], f[(i + 1) % f.length]); facets.set(k, [...(facets.get(k) || []), n]); } }
      const crease = (a, b) => { const ns = facets.get(key(a, b)); return !ns || ns.length < 2 || vec.dot(ns[0], ns[1]) < 0.9; };
      const seam = (a, b) => (m.seams || []).some(([p, q]) => (Math.hypot(...vec.sub(p, a)) < 1e-3 && Math.hypot(...vec.sub(q, b)) < 1e-3) || (Math.hypot(...vec.sub(p, b)) < 1e-3 && Math.hypot(...vec.sub(q, a)) < 1e-3));
      for (const f of m.faces) {
        const n = normal(f), k = 0.62 + 0.38 * vec.dot(n, light);
        const c = base.map((x) => Math.round(Math.min(255, x * k)));
        for (const t of triangles(f, n)) {
          for (const q of t) { pos.push(...q); col.push(...c, 255); }
          owner.push(mi);
        }
        if (edges) for (let i = 0; i < f.length; i++) {
          const a = f[i], b = f[(i + 1) % f.length];
          if (seam(a, b) || (m.smooth && !crease(a, b))) continue;
          lpos.push(...a, ...b); lcol.push(40, 40, 40, 170, 40, 40, 40, 170);
        }
      }
    });
    // The deflected shape, over the parts: each member's line, moved by its displacement times the exaggeration.
    const shape = deflected && deflected.members.length && part("deflectOn").checked ? deflected : null;
    if (shape) {
      let size = 0;
      const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
      for (let i = 0; i < pos.length; i++) { lo[i % 3] = Math.min(lo[i % 3], pos[i]); hi[i % 3] = Math.max(hi[i % 3], pos[i]); }
      size = Math.hypot(...vec.sub(hi, lo)) / 2;
      // (The slider's middle draws the largest displacement as about a tenth of the model.)
      const k = shape.max > 0 ? (0.1 * size / shape.max) * (Number(part("deflectScale").value) / 50) ** 2 : 0;
      part("deflectInfo").textContent = `×${k >= 10 ? k.toFixed(0) : k.toFixed(1)} · ${(shape.max * 1000).toFixed(2)} mm · ${shape.combination}`;
      for (const m of shape.members) {
        if (!objects.memberVisible(m.path)) continue;
        const at = (i) => [m.pts[i][0] + k * m.disp[i][0], m.pts[i][1] + k * m.disp[i][1], m.pts[i][2] + k * m.disp[i][2]];
        for (let i = 0; i + 1 < m.pts.length; i++) { lpos.push(...at(i), ...at(i + 1)); lcol.push(...DEFLECTED, ...DEFLECTED); }
      }
    }
    tris = new Float32Array(pos);
    view.setModel(tris, new Uint8Array(col), new Float32Array(lpos), new Uint8Array(lcol));
  }

  function fit() {
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (let i = 0; i < tris.length; i++) { lo[i % 3] = Math.min(lo[i % 3], tris[i]); hi[i % 3] = Math.max(hi[i % 3], tris[i]); }
    if (!(lo[0] <= hi[0])) return;
    view.frame(vec.scale(vec.add(lo, hi), 0.5), Math.hypot(...vec.sub(hi, lo)) / 2);
  }

  /** The part a ray meets first (its place among the mesh's), or null. */
  function hit({ origin, dir }) {
    let best = Infinity, at = null;
    for (let t = 0; t < owner.length; t++) {
      const a = tris.subarray(t * 9, t * 9 + 3), e1 = vec.sub(tris.subarray(t * 9 + 3, t * 9 + 6), a), e2 = vec.sub(tris.subarray(t * 9 + 6, t * 9 + 9), a);
      const h = vec.cross(dir, e2), det = vec.dot(e1, h);
      if (Math.abs(det) < 1e-12) continue;
      const s = vec.sub(origin, a), u = vec.dot(s, h) / det;
      if (u < 0 || u > 1) continue;
      const q = vec.cross(s, e1), v = vec.dot(dir, q) / det;
      if (v < 0 || u + v > 1) continue;
      const d = vec.dot(e2, q) / det;
      if (d > 1e-6 && d < best) { best = d; at = owner[t]; }
    }
    return at;
  }

  /** Says what is picked, beside the view's controls. */
  function say() {
    const m = picked == null ? null : mesh?.members[picked];
    // (What it is, shortly: the last of its path, and its material without the note of where its weight is from.)
    what.textContent = m ? `${m.path.split("/").slice(-2).join("/").split(":")[0]}${m.material ? ` · ${m.material.split(" (")[0]}` : ""}` : "";
    what.title = m ? `${m.path}${m.material ? `\n${m.material}` : ""}` : "";
  }

  function start() {
    view = new GLView(host);
    view.set({ solid: true, paper: true, az: 225, el: 30, layers: { scan: false } }, true);
    // A part picked in the view is chosen in the tree of objects, as a click on its row is.
    view.onPick = (ray) => {
      const at = hit(ray);
      if (at == null) { picked = null; say(); build(); objects.render(); return; }
      objects.select("member", mesh.members[at].path);
    };
    bar.querySelector('[data-a="fit"]').onclick = fit;
    bar.querySelector('[data-a="edges"]').onchange = (e) => { edges = e.target.checked; build(); };
    bar.querySelector('[data-a="ortho"]').onchange = (e) => view.set({ ortho: e.target.checked }, true);
    part("deflectOn").onchange = build;
    part("deflectScale").oninput = build;
  }

  /** The mesh of the model as it now is (for the tree of objects, and for the view if it is shown). */
  async function sync() {
    const v = app.version();
    if (v == null) return false;
    if (mesh && v === version) return false;
    mesh = await app.api("/api/mesh");
    version = v;
    picked = null;
    objects.render();
    return true;
  }

  /** The deflected shape for the combination chosen, if the model has one (a frame analysis); whether it changed. */
  async function shape() {
    const want = `${version}|${app.combination?.() ?? ""}`;
    if (want === deflectedFor) return false;
    deflectedFor = want;
    deflected = app.deflects?.() ? await app.api(`/api/deflected?combo=${encodeURIComponent(app.combination?.() ?? "")}`) : null;
    if (deflected?.error) deflected = null;
    part("deflect").hidden = !(deflected && deflected.members.length);
    return true;
  }

  async function load() {
    const fresh = await sync();
    if (!mesh) return;
    if (!view) start();
    const moved = await shape();
    if (!fresh && !moved && tris) return view.redraw();
    say();
    build();
    // (Another model: looked at whole. The same one, changed: the view is kept.)
    if (mesh.version_name !== name) { name = mesh.version_name; fit(); }
  }

  // The model's parts, in the tree of objects (a scan workspace lists them itself, with what the scan shows of each).
  objects.model({
    parts: () => mesh?.members ?? [],
    decorate: (m) => ({ title: `${m.path}${m.material ? ` — ${m.material.split(" (")[0]}` : ""}` }),
    selected: () => (picked == null ? null : mesh?.members[picked]?.path),
  });
  objects.on("select", (kind, path) => {
    if (kind !== "member" || !mesh) return;
    const at = mesh.members.findIndex((m) => m.path === path);
    picked = at < 0 ? null : at;
    say();
    if (view && tris) build();
    // (A member's checks; a solid has none.)
    if (at >= 0 && !mesh.members[at].material && !app.scan?.()) app.inspect(path);
  });
  objects.on("goto", (kind, id) => {
    if (!view || !mesh || !app.shown("shaded") || (kind !== "member" && kind !== "group")) return;
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (const m of mesh.members) {
      if (kind === "member" ? m.path !== id : !objects.memberParts(m.path).groups.join("/").startsWith(id)) continue;
      for (const f of m.faces) for (const q of f) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], q[k]); hi[k] = Math.max(hi[k], q[k]); }
    }
    if (lo[0] <= hi[0]) view.frame(vec.scale(vec.add(lo, hi), 0.5), Math.max(0.4, Math.hypot(...vec.sub(hi, lo)) / 2));
  });
  objects.on("visibility", () => { if (view && mesh && tris) build(); });

  app.registerPane("shaded", { label: "3D, shaded", el: "shadedPane", shown: () => { load(); app.showBrowser("layers"); } });
  return { load, sync };
}
