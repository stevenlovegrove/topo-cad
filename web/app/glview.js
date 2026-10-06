// A WebGL2 view of a scan with a model over it: points (coloured by
// intensity, height, surface normal or offset from a plane), the model's
// members (faces and edges), marks (lines), and labels as HTML over the
// canvas. Everything is in the scan's frame (z up, metres).
//
// One class draws both kinds of view:
//   "3d"       an orbit camera (perspective or orthographic) about a target;
//   "section"  an orthographic camera looking along −n at the slab about a
//              plane (points within `half` of it), panned and zoomed in the
//              plane. Sections are views of the real points, not images.
//
// `state` is the view state: plain data, so it can be saved, linked to and
// restored. `set(patch)` changes only what the patch names.

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mulS = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a) => { const n = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / n, a[1] / n, a[2] / n]; };
export const vec = { sub, add, scale: mulS, dot, cross, unit };

// Column-major 4×4 matrices.
function matMul(a, b) {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) o[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
  return o;
}
function viewMatrix(eye, s, u, f) {
  return new Float32Array([s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1]);
}
function perspective(fovy, aspect, near, far) {
  const f = 1 / Math.tan(fovy / 2);
  return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, (2 * far * near) / (near - far), 0]);
}
function orthographic(hw, hh, near, far) {
  return new Float32Array([1 / hw, 0, 0, 0, 0, 1 / hh, 0, 0, 0, 0, -2 / (far - near), 0, 0, 0, -(far + near) / (far - near), 1]);
}

const FOVY = (45 * Math.PI) / 180;
const D2R = Math.PI / 180;
export const COLOURS = ["intensity", "height", "normal", "depth", "confidence"];

const POINT_VS = `#version 300 es
in vec3 a_pos; in float a_int; in vec3 a_nrm; in float a_conf;
uniform mat4 u_vp; uniform float u_size; uniform float u_persp; uniform int u_mode; uniform int u_doubt;
uniform vec2 u_z; uniform vec3 u_bc; uniform mat3 u_bax; uniform vec3 u_bh; uniform float u_box; uniform vec3 u_toEye; uniform float u_floor;
out vec3 v_col;
vec3 hue(float h) { return clamp(abs(fract(h + vec3(0., 2. / 3., 1. / 3.)) * 6. - 3.) - 1., 0., 1.); }
void main() {
  // Where the point is in the box the view is clipped to (along its right, up and toward-the-viewer axes).
  vec3 q = (a_pos - u_bc) * u_bax;
  float off = q.z;
  // A point the rays or the lidar's flags doubt (under half): shown, faded or left out.
  bool doubt = a_conf < .5;
  if (a_pos.z < u_z.x || a_pos.z > u_z.y || (u_box > 0. && any(greaterThan(abs(q), u_bh))) || (doubt && u_doubt == 2)) { gl_Position = vec4(2., 2., 2., 1.); gl_PointSize = 0.; return; }
  gl_Position = u_vp * vec4(a_pos, 1.);
  gl_PointSize = u_persp > 0. ? clamp(u_size * 6. / max(gl_Position.w, .5), 1., u_size * 2.) : u_size;
  bool surf = dot(a_nrm, a_nrm) > .2;
  // Lit a little by how squarely the surface faces the viewer.
  float lit = surf ? .55 + .45 * abs(dot(normalize(a_nrm), u_toEye)) : .8;
  if (u_mode == 1) v_col = hue(.66 - .66 * clamp((a_pos.z - u_floor) / 3.6, 0., 1.)) * (.35 + .65 * lit);
  else if (u_mode == 2) v_col = surf ? .2 + .8 * abs(normalize(a_nrm)) : vec3(.42);
  else if (u_mode == 3) { float t = u_box > 0. ? clamp(off / u_bh.z, -1., 1.) : 0.; v_col = hue(.33 - .33 * t) * (.55 + .45 * lit); }
  else if (u_mode == 4) v_col = hue(.33 * a_conf) * (.55 + .45 * lit);
  else if (u_mode == 9) v_col = vec3(1., .82, .1);
  else v_col = vec3(.12 + .88 * pow(a_int, .55)) * vec3(1., .98, .94) * lit;
  if (doubt && u_doubt == 1 && u_mode != 4) { v_col *= .3; gl_PointSize = max(1., .6 * gl_PointSize); }
}`;
const POINT_FS = `#version 300 es
precision mediump float;
in vec3 v_col; out vec4 o;
void main() { vec2 d = gl_PointCoord - .5; if (dot(d, d) > .25) discard; o = vec4(v_col, 1.); }`;
const FLAT_VS = `#version 300 es
in vec3 a_pos; in vec4 a_col;
uniform mat4 u_vp;
out vec4 v_col; out vec3 v_pos;
void main() { gl_Position = u_vp * vec4(a_pos, 1.); v_col = a_col; v_pos = a_pos; }`;
const FLAT_FS = `#version 300 es
precision mediump float;
in vec4 v_col; in vec3 v_pos; out vec4 o;
uniform vec2 u_z; uniform vec3 u_bc; uniform mat3 u_bax; uniform vec3 u_bh; uniform float u_box;
void main() {
  if (v_pos.z < u_z.x || v_pos.z > u_z.y) discard;
  if (u_box > 0. && any(greaterThan(abs((v_pos - u_bc) * u_bax), u_bh))) discard;
  o = v_col;
}`;
// A picture laid in the scene (free space over a slab): one value per cell, shown as a tint.
const IMAGE_VS = `#version 300 es
in vec3 a_pos; in vec2 a_uv;
uniform mat4 u_vp;
out vec2 v_uv;
void main() { gl_Position = u_vp * vec4(a_pos, 1.); v_uv = a_uv; }`;
const IMAGE_FS = `#version 300 es
precision mediump float;
in vec2 v_uv; out vec4 o;
uniform sampler2D u_tex;
void main() {
  float a = texture(u_tex, v_uv).r;
  // Few rays: a dim blue; many: a bright cyan.
  o = vec4(mix(vec3(.08, .22, .6), vec3(.35, .85, 1.), a), a > 0. ? .25 + .6 * a : 0.);
}`;

function program(gl, vs, fs) {
  const p = gl.createProgram();
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    gl.attachShader(p, s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  const u = {};
  for (let i = 0; i < gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS); i++) {
    const name = gl.getActiveUniform(p, i).name;
    u[name] = gl.getUniformLocation(p, name);
  }
  return { p, u };
}

export function defaultState() {
  return {
    // An orbit about `target`, seen from compass bearing `az`, `el` up (degrees).
    target: [0, 0, 1.2], az: 215, el: 28, dist: 14, ortho: false, half: 6,
    // The box the view is clipped to (a slab: { c, u, v, n, h }, scan frame), and
    // whether a drag slides the view (seen square on) rather than turning it.
    box: null, flat: false,
    // Whether free space (where the rays pass in the box) is shown under the points.
    free: false,
    // Whether a window dragged in the view takes every point in it, not only the surface nearest the viewer.
    selectAll: false,
    colour: "intensity",
    // Points the rays or the lidar's flags doubt (in a slab): "show", "fade" or "hide".
    doubt: "fade",
    // Height cuts, in the model's frame (null: none).
    zMin: null, zMax: null,
    layers: { scan: true, model: true, marks: true, issues: true },
    // The model drawn solid (its faces hide what is behind them), not as glass over the points; on paper, not on black.
    solid: false, paper: false,
    pointSize: 2,
  };
}

export class GLView {
  constructor(host) {
    this.host = host;
    this.canvas = document.createElement("canvas");
    this.canvas.style.cssText = "width:100%;height:100%;display:block;touch-action:none";
    this.overlay = document.createElement("div");
    this.overlay.style.cssText = "position:absolute;inset:0;pointer-events:none;overflow:hidden";
    host.append(this.canvas, this.overlay);
    const gl = (this.gl = this.canvas.getContext("webgl2", { antialias: true, preserveDrawingBuffer: true }));
    if (!gl) throw new Error("this browser has no WebGL2");
    this.points = program(gl, POINT_VS, POINT_FS);
    this.flat = program(gl, FLAT_VS, FLAT_FS);
    this.pic = program(gl, IMAGE_VS, IMAGE_FS);
    this.image = null;
    this.state = defaultState();
    this.handles = [];
    this.handleEls = new Map();
    /** "line": the next drag draws a line (onLine gets the rays at its ends). */
    this.tool = null;
    this.rule = document.createElement("div");
    this.rule.style.cssText = "position:absolute;height:0;border-top:2px dashed #ff9628;transform-origin:0 0;display:none";
    this.overlay.appendChild(this.rule);
    /** The model's floor in the scan (model z = scan z − zOffset). */
    this.zOffset = 0;
    this.floorZ = 0;
    this.clouds = { thin: null, slab: null };
    this.highlights = new Map();
    this.tris = null; this.lines = null; this.marks = null;
    this.labels = [];
    this.labelEls = new Map();
    this.onPick = null; this.onChange = null; this.onLabel = null; this.onSelect = null;
    this.band = document.createElement("div");
    this.band.style.cssText = "position:absolute;border:1px dashed #ffd21a;background:rgba(255,210,26,.12);display:none";
    this.overlay.appendChild(this.band);
    this.dirty = false;
    this.bind();
    new ResizeObserver(() => this.redraw()).observe(host);
    this.redraw();
  }

  // ----- state --------------------------------------------------------------------

  /** Sets what `patch` names (layers merge), and redraws. */
  set(patch, quiet) {
    const { layers, ...rest } = patch;
    Object.assign(this.state, structuredClone(rest));
    if (layers) Object.assign(this.state.layers, layers);
    this.redraw();
    if (!quiet) this.onChange?.();
  }
  get() {
    return structuredClone(this.state);
  }
  redraw() {
    if (this.dirty) return;
    this.dirty = true;
    requestAnimationFrame(() => this.draw());
  }

  // ----- data ---------------------------------------------------------------------

  cloudBuffers(c) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const attr = (name, data, size, type, norm) => {
      const loc = gl.getAttribLocation(this.points.p, name);
      gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, type, norm, 0, 0);
    };
    attr("a_pos", c.xyz, 3, gl.FLOAT, false);
    attr("a_int", c.intensity, 1, gl.UNSIGNED_BYTE, true);
    attr("a_nrm", c.normal, 3, gl.BYTE, true);
    attr("a_conf", c.confidence ?? new Uint8Array(c.count).fill(255), 1, gl.UNSIGNED_BYTE, true);
    gl.bindVertexArray(null);
    return { vao, count: c.count };
  }
  /** The thinned scan (`which` "thin") or a section's slab ("slab"; null drops it). */
  /** A set of points to draw highlighted, by key (null drops it). */
  setHighlight(key, cloud) {
    if (cloud) this.highlights.set(key, this.cloudBuffers(cloud));
    else this.highlights.delete(key);
    this.redraw();
  }
  setCloud(which, cloud) {
    this.clouds[which] = cloud ? this.cloudBuffers(cloud) : null;
    this.redraw();
  }

  flatBuffers(pos, col) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const a = gl.getAttribLocation(this.flat.p, "a_pos"), c = gl.getAttribLocation(this.flat.p, "a_col");
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(a);
    gl.vertexAttribPointer(a, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, col, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(c);
    gl.vertexAttribPointer(c, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    gl.bindVertexArray(null);
    return { vao, count: pos.length / 3 };
  }
  /** The model: faces as triangles and edges as line segments, each vertex [x, y, z] with an RGBA colour. */
  setModel(tris, triCol, lines, lineCol) {
    this.tris = tris.length ? this.flatBuffers(tris, triCol) : null;
    this.lines = lines.length ? this.flatBuffers(lines, lineCol) : null;
    this.redraw();
  }
  /** Marks: line segments (boxes, a section's plane, picks, surfaces). */
  setMarks(pos, col) {
    this.marks = pos.length ? this.flatBuffers(pos, col) : null;
    this.redraw();
  }
  /**
   * A picture laid in the scene, under the points: { corners, w, h, data }
   * (corners: where its low-low, high-low, low-high and high-high corners
   * are; a byte per cell, row 0 at the low side). Null drops it.
   */
  setImage(img) {
    const gl = this.gl;
    if (this.image) { gl.deleteTexture(this.image.tex); gl.deleteVertexArray(this.image.vao); }
    this.image = null;
    if (img) {
      const tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, img.w, img.h, 0, gl.RED, gl.UNSIGNED_BYTE, img.data);
      for (const [k, x] of [[gl.TEXTURE_MIN_FILTER, gl.LINEAR], [gl.TEXTURE_MAG_FILTER, gl.LINEAR], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE]]) gl.texParameteri(gl.TEXTURE_2D, k, x);
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const attr = (name, data, size) => {
        const loc = gl.getAttribLocation(this.pic.p, name);
        gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STATIC_DRAW);
        gl.enableVertexAttribArray(loc);
        gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
      };
      attr("a_pos", img.corners.flat(), 3);
      attr("a_uv", [0, 0, 1, 0, 0, 1, 1, 1], 2);
      gl.bindVertexArray(null);
      this.image = { tex, vao };
    }
    this.redraw();
  }
  /** Faint surfaces (a selected slab's faces, a plane's patch): triangles, each vertex with an RGBA colour. Points in front of one hide it; those behind are tinted. */
  setFills(pos, col) {
    this.fills = pos.length ? this.flatBuffers(pos, col) : null;
    this.redraw();
  }
  /** Labels at places: [{ id, pos, text, colour, title, on }]. */
  setLabels(labels) {
    this.labels = labels;
    for (const [id, el] of this.labelEls) if (!labels.some((l) => l.id === id)) { el.remove(); this.labelEls.delete(id); }
    for (const l of labels) {
      let el = this.labelEls.get(l.id);
      if (!el) {
        el = document.createElement("div");
        // A dot at the place, and the label set off from it on a leader, so it does not cover what it points at.
        el.className = "pin";
        el.innerHTML = `<i class="dot"></i><i class="lead"></i><span class="tag"></span>`;
        el.querySelector(".tag").ondblclick = (e) => { e.stopPropagation(); this.onLabelZoom?.(l.id); };
        el.querySelector(".tag").onclick = (e) => { e.stopPropagation(); this.onLabel?.(l.id); };
        el.querySelector(".tag").onpointerdown = (e) => e.stopPropagation();
        this.overlay.appendChild(el);
        this.labelEls.set(l.id, el);
      }
      el.querySelector(".tag").textContent = l.text;
      el.title = l.title || "";
      el.style.setProperty("--pin", l.colour || "#1f5fbf");
      el.classList.toggle("on", !!l.on);
      el.classList.toggle("dim", !!l.dim);
    }
    this.redraw();
  }
  /**
   * Handles to drag, at places: [{ id, pos, cls, text, title, along }].
   * `onHandle(id, "start" | "move" | "end", ray, event)` as one is dragged;
   * `onHandleMenu(id, event)` on a right-click. One whose `along` (the line
   * it moves on) points at the viewer is not shown: it could not be dragged.
   */
  setHandles(list) {
    this.handles = list;
    for (const [id, el] of this.handleEls) if (!list.some((h) => h.id === id) && !el.dragging) { el.remove(); this.handleEls.delete(id); }
    for (const h of list) {
      let el = this.handleEls.get(h.id);
      if (!el) {
        el = document.createElement("div");
        const at = (e) => { const r = this.canvas.getBoundingClientRect(); return this.ray(e.clientX - r.left, e.clientY - r.top); };
        el.onpointerdown = (e) => {
          e.stopPropagation();
          if (e.button !== 0) return;
          e.preventDefault();
          el.setPointerCapture(e.pointerId);
          el.dragging = true;
          this.onHandle?.(h.id, "start", at(e), e);
        };
        el.onpointermove = (e) => { if (el.dragging) this.onHandle?.(h.id, "move", at(e), e); };
        el.onpointerup = (e) => { if (el.dragging) { el.dragging = false; this.onHandle?.(h.id, "end", at(e), e); } };
        el.oncontextmenu = (e) => { e.preventDefault(); e.stopPropagation(); this.onHandleMenu?.(h.id, e); };
        this.overlay.appendChild(el);
        this.handleEls.set(h.id, el);
      }
      el.className = `hdl ${h.cls || ""}`;
      el.textContent = h.text || "";
      el.title = h.title || "";
    }
    this.redraw();
  }

  // ----- camera -------------------------------------------------------------------

  /** The camera: view-projection matrix, basis (right, up, forward), eye. */
  camera() {
    const s = this.state, w = this.canvas.clientWidth || 1, h = this.canvas.clientHeight || 1, aspect = w / h;
    const az = s.az * D2R, el = Math.max(-89.9, Math.min(89.9, s.el)) * D2R;
    const t = [Math.sin(az) * Math.cos(el), Math.cos(az) * Math.cos(el), Math.sin(el)];  // toward the viewer
    const f = mulS(t, -1);
    // Looking straight down: the bearing is what is "behind" the viewer, so up is away from it.
    let right = cross(f, [0, 0, 1]);
    right = Math.hypot(...right) < 1e-3 ? [-Math.cos(az), Math.sin(az), 0] : unit(right);
    const up = cross(right, f);
    if (s.ortho) {
      const eye = add(s.target, mulS(t, 200));
      return { vp: matMul(orthographic(s.half * aspect, s.half, 1, 400), viewMatrix(eye, right, up, f)), right, up, forward: f, eye, ortho: true, hh: s.half, hw: s.half * aspect, w, h };
    }
    const eye = add(s.target, mulS(t, s.dist));
    return { vp: matMul(perspective(FOVY, aspect, Math.max(0.05, s.dist / 200), s.dist * 20 + 200), viewMatrix(eye, right, up, f)), right, up, forward: f, eye, ortho: false, w, h };
  }

  /** Where a point is on the canvas (CSS pixels), or null if behind the viewer. */
  project(cam, p) {
    const m = cam.vp;
    const x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    if (w <= 1e-6) return null;
    return [(x / w + 1) * 0.5 * cam.w, (1 - y / w) * 0.5 * cam.h];
  }

  /** The ray through a canvas position: { origin, dir }. */
  ray(x, y) {
    const cam = this.camera();
    const nx = (2 * x) / cam.w - 1, ny = 1 - (2 * y) / cam.h;
    if (cam.ortho) return { origin: add(cam.eye, add(mulS(cam.right, nx * cam.hw), mulS(cam.up, ny * cam.hh))), dir: cam.forward };
    const th = Math.tan(FOVY / 2);
    return { origin: cam.eye, dir: unit(add(cam.forward, add(mulS(cam.right, nx * th * (cam.w / cam.h)), mulS(cam.up, ny * th)))) };
  }

  /** The cuts in force, as the service's pick takes them. */
  cuts() {
    const s = this.state;
    const z = s.zMin == null && s.zMax == null ? undefined : [s.zMin ?? -1e9, s.zMax ?? 1e9];
    return { z, slab: s.box || undefined };
  }

  /** Frames a sphere (keeping the direction of view). */
  frame(centre, radius) {
    this.animate({ target: centre, dist: Math.max(1.5, radius / Math.sin(FOVY / 2) * 1.1), half: Math.max(0.3, radius * 1.2) });
  }

  /** Looks square on at a box (from its `n` side, its `v` up), fitted in the view; a drag then slides it. */
  squareOn(box, ms) {
    const { c, v, n, h } = box, aspect = (this.canvas.clientWidth || 4) / (this.canvas.clientHeight || 3);
    // Seen from above (or below), the bearing is what puts the box's `v` up.
    const flat = Math.abs(n[2]) > 0.999;
    this.animate({
      target: c, az: (flat ? Math.atan2(-v[0] * n[2], -v[1] * n[2]) : Math.atan2(n[0], n[1])) / D2R, el: Math.asin(Math.max(-1, Math.min(1, n[2]))) / D2R,
      half: 1.06 * Math.max(h[1], h[0] / aspect), dist: Math.max(this.state.dist, 2), ortho: true, flat: true,
    }, ms);
  }

  /**
   * Goes to a view smoothly, so it is seen where it went: the camera's
   * numbers in `patch` (target, az, el, dist, half, centre, zoom) are eased
   * to; anything else in it is set at once. A drag or a wheel stops it.
   */
  animate(patch, ms = 450) {
    const s = this.state, from = {}, to = {}, rest = {};
    for (const [k, v] of Object.entries(patch)) {
      if (["target", "az", "el", "dist", "half", "centre", "zoom"].includes(k) && v != null) { from[k] = structuredClone(s[k]); to[k] = v; }
      else rest[k] = v;
    }
    // A bearing turns the short way round.
    if ("az" in to) to.az = from.az + ((((to.az - from.az) % 360) + 540) % 360 - 180);
    if (Object.keys(rest).length) this.set(rest, true);
    // (Nothing of the camera's named: a flight under way goes on.)
    if (!Object.keys(to).length) return this.onChange?.();
    cancelAnimationFrame(this.anim);
    const t0 = performance.now(), scaled = new Set(["dist", "half", "zoom"]);
    const step = (now) => {
      const x = Math.min(1, (now - t0) / ms), e = x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
      const p = {};
      for (const k of Object.keys(to)) {
        const a = from[k], b = to[k];
        p[k] = Array.isArray(a) ? a.map((v, i) => v + (b[i] - v) * e) : scaled.has(k) ? a * Math.pow(b / a, e) : a + (b - a) * e;
      }
      this.set(p, true);
      if (x < 1) this.anim = requestAnimationFrame(step);
      else this.onChange?.();
    };
    this.anim = requestAnimationFrame(step);
  }

  bind() {
    const c = this.canvas;
    let drag = null;
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("pointerdown", (e) => {
      try { c.setPointerCapture(e.pointerId); } catch (_) {}
      cancelAnimationFrame(this.anim);
      // Shift-drag marks a window of points; right- or alt-drag pans (a view held square on pans with any drag).
      const line = this.tool === "line" && e.button === 0;
      drag = { x: e.clientX, y: e.clientY, line, select: !line && e.shiftKey && e.button === 0, pan: e.button === 2 || e.altKey || this.state.flat, moved: false, s: this.get() };
    });
    c.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      if (!drag.moved) return;
      if (drag.select) {
        const r = c.getBoundingClientRect();
        Object.assign(this.band.style, { display: "block", left: `${Math.min(drag.x, e.clientX) - r.left}px`, top: `${Math.min(drag.y, e.clientY) - r.top}px`, width: `${Math.abs(dx)}px`, height: `${Math.abs(dy)}px` });
        return;
      }
      if (drag.line) {
        const r = c.getBoundingClientRect();
        Object.assign(this.rule.style, { display: "block", left: `${drag.x - r.left}px`, top: `${drag.y - r.top}px`, width: `${Math.hypot(dx, dy)}px`, transform: `rotate(${Math.atan2(dy, dx)}rad)` });
        return;
      }
      const s0 = drag.s, cam = this.camera();
      if (drag.pan) {
        const k = s0.ortho ? (2 * s0.half) / cam.h : (2 * s0.dist * Math.tan(FOVY / 2)) / cam.h;
        this.set({ target: add(s0.target, add(mulS(cam.right, -dx * k), mulS(cam.up, dy * k))) }, true);
      } else {
        this.set({ az: s0.az - dx * 0.35, el: Math.max(-89.9, Math.min(89.9, s0.el + dy * 0.35)) }, true);
      }
    });
    c.addEventListener("pointerup", (e) => {
      const d = drag;
      drag = null;
      if (!d) return;
      const r = c.getBoundingClientRect();
      if (d.line) {
        this.rule.style.display = "none";
        if (d.moved) this.onLine?.(this.ray(d.x - r.left, d.y - r.top), this.ray(e.clientX - r.left, e.clientY - r.top));
        return;
      }
      if (d.select) {
        this.band.style.display = "none";
        if (!d.moved) return;
        // The window, as the view projects it: what the service needs to find the points in it.
        const cam = this.camera(), nx = (x) => (2 * (x - r.left)) / cam.w - 1, ny = (y) => 1 - (2 * (y - r.top)) / cam.h;
        this.onSelect?.({ vp: Array.from(cam.vp), rect: [nx(d.x), ny(d.y), nx(e.clientX), ny(e.clientY)], eye: cam.eye, forward: cam.forward }, e);
        return;
      }
      if (d.moved) this.onChange?.();
      else if (e.button === 0) this.onPick?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
      // A right-click (not a pan): the host's menu for what is under the cursor.
      else if (e.button === 2) this.onContext?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
    });
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      cancelAnimationFrame(this.anim);
      const f = Math.exp(e.deltaY * 0.0015), s = this.state;
      {
        // Zoom toward what is under the cursor, so it stays there: the scan point
        // it is on (asked of the host, once per place the cursor rests), or until
        // that is known, the point at the depth of the view's target.
        const r = c.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top, ray = this.ray(x, y), now = performance.now();
        const z = this.zoomAt;
        if (!z || Math.hypot(z.x - x, z.y - y) > 12 || now - z.t > 1500) {
          const fresh = (this.zoomAt = { x, y, t: now, p: null });
          this.onDepth?.(ray).then((p) => { if (p) fresh.p = p; }).catch(() => {});
        } else z.t = now;
        const cam = this.camera(), depth = dot(sub(s.target, cam.eye), cam.forward);
        const p = this.zoomAt.p ?? add(ray.origin, mulS(ray.dir, cam.ortho ? depth : depth / Math.max(0.2, dot(ray.dir, cam.forward))));
        const target = add(p, mulS(sub(s.target, p), f));
        this.set(s.ortho ? { half: s.half * f, target } : { dist: Math.max(0.2, s.dist * f), target }, true);
      }
      clearTimeout(this.wheelTimer);
      this.wheelTimer = setTimeout(() => this.onChange?.(), 250);
    }, { passive: false });
  }

  // ----- drawing ------------------------------------------------------------------

  draw() {
    this.dirty = false;
    const gl = this.gl, c = this.canvas, dpr = window.devicePixelRatio || 1;
    const w = Math.round(c.clientWidth * dpr), h = Math.round(c.clientHeight * dpr);
    if (!w || !h) return;
    if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    gl.viewport(0, 0, w, h);
    const s = this.state, cam = this.camera();
    if (s.paper) gl.clearColor(0.97, 0.97, 0.955, 1);
    else gl.clearColor(0.085, 0.09, 0.105, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    const z = [s.zMin == null ? -1e9 : s.zMin + this.zOffset, s.zMax == null ? 1e9 : s.zMax + this.zOffset];
    const box = s.box;
    const cuts = (u) => {
      gl.uniformMatrix4fv(u.u_vp, false, cam.vp);
      gl.uniform2f(u.u_z, z[0], z[1]);
      gl.uniform1f(u.u_box, box ? 1 : 0);
      if (box) {
        gl.uniform3f(u.u_bc, ...box.c);
        gl.uniformMatrix3fv(u.u_bax, false, [...box.u, ...box.v, ...box.n]);
        gl.uniform3f(u.u_bh, ...box.h);
      }
    };
    // A picture under everything (free space): no depth, so the points are drawn over it.
    if (this.image && s.layers.scan) {
      gl.useProgram(this.pic.p);
      gl.disable(gl.DEPTH_TEST);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.uniformMatrix4fv(this.pic.u.u_vp, false, cam.vp);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.image.tex);
      gl.uniform1i(this.pic.u.u_tex, 0);
      gl.bindVertexArray(this.image.vao);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
    }
    // Clipped to a box, every point of the scan in it is drawn (once they have come).
    const cloud = box ? this.clouds.slab || this.clouds.thin : this.clouds.thin;
    if (s.layers.scan && cloud) {
      const u = this.points.u;
      gl.useProgram(this.points.p);
      cuts(u);
      gl.uniform1f(u.u_size, s.pointSize * dpr);
      gl.uniform1f(u.u_persp, cam.ortho ? 0 : 1);
      gl.uniform1i(u.u_mode, Math.max(0, COLOURS.indexOf(s.colour)));
      gl.uniform1i(u.u_doubt, { show: 0, fade: 1, hide: 2 }[s.doubt] ?? 1);
      gl.uniform3f(u.u_toEye, -cam.forward[0], -cam.forward[1], -cam.forward[2]);
      gl.uniform1f(u.u_floor, this.floorZ);
      gl.bindVertexArray(cloud.vao);
      gl.drawArrays(gl.POINTS, 0, cloud.count);
    }
    // Highlighted sets of points (selections), over the others; shown whether or not the scan is.
    if (this.highlights.size) {
      const u = this.points.u;
      gl.useProgram(this.points.p);
      cuts(u);
      gl.uniform1f(u.u_persp, cam.ortho ? 0 : 1);
      gl.uniform1i(u.u_mode, 9);
      gl.uniform1i(u.u_doubt, 0);
      gl.uniform1f(u.u_size, (s.pointSize + 1) * dpr);
      for (const h of this.highlights.values()) { gl.bindVertexArray(h.vao); gl.drawArrays(gl.POINTS, 0, h.count); }
    }
    gl.useProgram(this.flat.p);
    cuts(this.flat.u);
    // In a slab the model is drawn over the points: it is what they are read against.
    if (box) gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    if (s.layers.model && this.tris && s.solid) {
      // Faces as they are, each hiding what is behind it (set back a little, so their edges lie on them).
      gl.disable(gl.BLEND);
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(1, 1);
      gl.bindVertexArray(this.tris.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.tris.count);
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.enable(gl.BLEND);
    } else if (s.layers.model && this.tris) {
      // Faces are translucent: seen through to the points, not hiding each other.
      gl.depthMask(false);
      gl.bindVertexArray(this.tris.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.tris.count);
      gl.depthMask(true);
    }
    if (s.layers.model && this.lines) {
      gl.bindVertexArray(this.lines.vao);
      gl.drawArrays(gl.LINES, 0, this.lines.count);
    }
    if (s.layers.marks && this.fills) {
      gl.enable(gl.DEPTH_TEST);
      gl.depthMask(false);
      gl.uniform1f(this.flat.u.u_box, 0);
      gl.uniform2f(this.flat.u.u_z, -1e9, 1e9);
      gl.bindVertexArray(this.fills.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.fills.count);
      gl.depthMask(true);
    }
    if (s.layers.marks && this.marks) {
      gl.disable(gl.DEPTH_TEST);
      gl.uniform1f(this.flat.u.u_box, 0);
      gl.uniform2f(this.flat.u.u_z, -1e9, 1e9);
      gl.bindVertexArray(this.marks.vao);
      gl.drawArrays(gl.LINES, 0, this.marks.count);
    }
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    // Labels: HTML over the canvas, where their places project to.
    for (const l of this.labels) {
      const el = this.labelEls.get(l.id);
      const p = s.layers.issues ? this.project(cam, l.pos) : null;
      const q = box && sub(l.pos, box.c);
      const cut = box && [box.u, box.v, box.n].some((a, k) => Math.abs(dot(a, q)) > Math.max(box.h[k], 0.3));
      if (!p || cut || p[0] < -40 || p[1] < -20 || p[0] > cam.w + 40 || p[1] > cam.h + 20) { el.style.display = "none"; continue; }
      el.style.display = "";
      el.style.transform = `translate(${p[0].toFixed(1)}px, ${p[1].toFixed(1)}px)`;
    }
    for (const h of this.handles) {
      const el = this.handleEls.get(h.id);
      if (!el) continue;
      const p = this.project(cam, h.pos);
      if (!p || (h.along && Math.abs(dot(h.along, cam.forward)) > 0.94) || p[0] < 0 || p[1] < 0 || p[0] > cam.w || p[1] > cam.h) { if (!el.dragging) el.style.display = "none"; continue; }
      el.style.display = "";
      el.style.transform = `translate(${p[0].toFixed(1)}px, ${p[1].toFixed(1)}px) translate(-50%, -50%)`;
    }
    // What the host draws over the view to match it (a compass).
    this.onDraw?.(cam);
  }

  /** The view as a PNG data URL (as drawn, without the labels). */
  png() {
    this.draw();
    return this.canvas.toDataURL("image/png");
  }
}
