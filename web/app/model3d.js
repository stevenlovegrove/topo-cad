// The model, shaded: a WebGL view (glview.js) of its parts, members and solids alike, each in
// the colour of what it is made of, hiding what is behind it. It needs no scan and no service:
// it is drawn from the mesh the engine gives the page (`/api/mesh`).

import { GLView, vec } from "./glview.js";

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
const TIMBER = "#d8b683", PICKED = "#ff3cdc";
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
 * Adds the pane ("3D, shaded"). `app`: { registerPane, api, version (of the model as built),
 * inspect (a member, by its path), parked (where panes not shown are kept) }. Returns
 * { load }: to be called when the model has changed and the pane is shown.
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
  bar.innerHTML = `<button data-a="fit">fit</button><label><input type="checkbox" data-a="edges" checked> edges</label><label><input type="checkbox" data-a="ortho"> parallel</label><span data-a="what" style="color:#444;max-width:46ch;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"></span>`;
  const hint = document.createElement("div");
  hint.style.cssText = "position:absolute;left:8px;bottom:6px;font:11px system-ui;color:#777;pointer-events:none";
  hint.textContent = "drag to orbit · right-drag to pan · scroll to zoom · click a part";
  pane.append(host, bar, hint);
  app.parked.appendChild(pane);

  let view = null, mesh = null, version = null, name = null, picked = null, tris = null, owner = null, edges = true;
  const what = bar.querySelector('[data-a="what"]');

  function build() {
    const light = vec.unit([0.35, -0.55, 0.76]);
    const pos = [], col = [], lpos = [], lcol = [];
    owner = [];
    mesh.members.forEach((m, mi) => {
      const base = rgb(mi === picked ? PICKED : m.material ? materialColour(m.material) : TIMBER);
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
          if (seam(a, b)) continue;
          lpos.push(...a, ...b); lcol.push(40, 40, 40, 170, 40, 40, 40, 170);
        }
      }
    });
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

  function start() {
    view = new GLView(host);
    view.set({ solid: true, paper: true, az: 225, el: 30, layers: { scan: false } }, true);
    view.onPick = (ray) => {
      picked = hit(ray);
      const m = picked == null ? null : mesh.members[picked];
      // (What it is, shortly: the last of its path, and its material without the note of where its weight is from.)
      what.textContent = m ? `${m.path.split("/").slice(-2).join("/").split(":")[0]}${m.material ? ` · ${m.material.split(" (")[0]}` : ""}` : "";
      what.title = m ? `${m.path}${m.material ? `\n${m.material}` : ""}` : "";
      build();
      if (m && !m.material) app.inspect(m.path);
    };
    bar.querySelector('[data-a="fit"]').onclick = fit;
    bar.querySelector('[data-a="edges"]').onchange = (e) => { edges = e.target.checked; build(); };
    bar.querySelector('[data-a="ortho"]').onchange = (e) => view.set({ ortho: e.target.checked }, true);
  }

  async function load() {
    const v = app.version();
    if (v == null) return;
    if (!view) start();
    if (mesh && v === version) return view.redraw();
    mesh = await app.api("/api/mesh");
    version = v;
    picked = null;
    what.textContent = "";
    build();
    // (Another model: looked at whole. The same one, changed: the view is kept.)
    if (mesh.version_name !== name) { name = mesh.version_name; fit(); }
  }

  app.registerPane("shaded", { label: "3D, shaded", el: "shadedPane", shown: () => load() });
  return { load };
}
