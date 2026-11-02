// What the page did to show a project, and when: for whoever asks why a load was slow (a person in
// the console: `topoTimes()`; the load test: web/tools/page.mjs). Times are milliseconds since the
// page began to load.
//
//   reached(what)       a thing has come about (the first time it does, for the project that is open):
//                       "state" (the model is built, and said), "parts" (they are in the 3D view),
//                       "files", "workspace", "sheets" (they are drawn, and listed)
//   asked(engine, op)   something is asked of an engine: answers with a function to call when it is
//                       answered (with how long the engine worked on it, which is not how long it took:
//                       it may have waited behind something else)
//   blocked             where the page itself could do nothing for 50 ms or more: nothing is drawn, and no
//                       click is heard, while its own thread is busy
//   step(source, what)  something (the page, or one of its engines) begins a step of its work: what it was doing
//                       before is done. `idle(source)`: it has stopped. The build log (`log()`, shown in the page
//                       as a document) is these, each with how long it took, and what is reached and blocked
//
// This module knows nothing of what is timed: whoever does a thing says so here.

const KEPT = 400;
/** What has come about, by name: when. And for which project. */
let reachedAt = {}, of = null;
const engines = [], blocked = [];
/** The build log: each step `{ at, source, what }` as it began (`what` null: the source stopped), in the order heard. */
let steps = [];
const LOG = 5000;
const listeners = new Set();
let saying = null;
/** Whoever shows the log is told it has more (once a frame, at most). */
const said = () => { saying ??= requestAnimationFrame(() => { saying = null; for (const f of listeners) f(); }); };

/** The project is being opened: what comes about from here is its. */
export function began(project) {
  of = project;
  reachedAt = { began: Math.round(performance.now()) };
  step("page", `opening ${project}`);
}
export function reached(what) {
  if (reachedAt[what] != null) return;
  reachedAt[what] = Math.round(performance.now());
  steps.push({ at: reachedAt[what], source: "page", what: `reached: ${what}`, mark: true });
  said();
}
/** `source` begins a step of its work, `what` (at `at`, ms since the page began; now, unless said). */
export function step(source, what, at = performance.now()) {
  steps.push({ at, source, what });
  if (steps.length > LOG) steps = steps.slice(-LOG);
  said();
}
/** `source` has stopped working (at `at`). */
export const idle = (source, at) => step(source, null, at);
/** `f()` whenever the log has more. Answers a function that stops that. */
export function onLog(f) {
  listeners.add(f);
  return () => listeners.delete(f);
}
/** The build log: each step with how long it took (till its source's next step, or till now where it is the last
 *  one: `running`), and the marks (what was reached), in the order they began. */
export function log() {
  const now = performance.now(), last = new Map(), out = [];
  for (const s of steps) {
    if (s.mark) { out.push({ ...s, ms: s.ms ?? 0 }); continue; }
    const open = last.get(s.source);
    if (open) open.ms = s.at - open.at, open.running = false;
    if (s.what == null) { last.delete(s.source); continue; }
    const row = { ...s, ms: now - s.at, running: true };
    out.push(row);
    last.set(s.source, row);
  }
  return out.sort((a, b) => a.at - b.at);
}
/** What each source is doing now (where it is doing something): `{ source: what }`. */
export function doing() {
  const now = {};
  for (const s of steps) if (!s.mark) now[s.source] = s.what;
  for (const k of Object.keys(now)) if (now[k] == null) delete now[k];
  return now;
}
export function asked(engine, op) {
  const row = { engine, op, asked: Math.round(performance.now()), answered: null, worked: null };
  engines.push(row);
  if (engines.length > KEPT) engines.shift();
  return (worked) => { row.answered = Math.round(performance.now()); row.worked = Math.round(worked ?? 0); };
}
try {
  new PerformanceObserver((list) => {
    for (const e of list.getEntries()) {
      blocked.push({ at: Math.round(e.startTime), for: Math.round(e.duration) });
      steps.push({ at: e.startTime, source: "page", what: "the page's thread busy (nothing drawn, no click heard)", mark: true, ms: e.duration });
    }
    said();
    if (blocked.length > KEPT) blocked.splice(0, blocked.length - KEPT);
  }).observe({ type: "longtask", buffered: true });
} catch (_) {
  // (A browser that does not say when its thread was busy: nothing is said of it.)
}

// The 3D view's frames: how long each took on the page's thread (`cpu`), and, of those drawn one after another (the
// view moving), how long from the one before (`gap`: where the GPU is what takes the time, this says so, not `cpu`).
const frameTimes = [];
export function frame(cpu, gap) {
  frameTimes.push({ cpu, gap });
  if (frameTimes.length > 600) frameTimes.shift();
}
// What the renderer's own counters said (`View::counters`: `{ name: [count, nanos, max_nanos] }`), as read now and
// then while frames are drawn: of the last few seconds, the GPU's time a frame (`gpu.frame`), and what the B-rep
// shader did a frame where it counts (`brep.*`).
let counted = [];
export function counters(said) {
  const now = performance.now();
  counted.push({ at: now, said });
  counted = counted.filter((c) => now - c.at < 4000);
}
/** What was counted is let go of (the switches changed: what is counted from here is of them). */
export const recount = () => { counted = []; };
/** Of the frames drawn last (up to 600): how many, and the median and worst of each time (ms); and of the last few
 *  seconds, the GPU's time a frame and, where counted, what the B-rep shader did a frame. */
export function frames() {
  const of = (xs) => { const v = xs.filter((x) => x != null).sort((a, b) => a - b); return v.length ? { median: v[v.length >> 1], worst: v[v.length - 1], n: v.length } : null; };
  const out = { n: frameTimes.length, cpu: of(frameTimes.map((f) => f.cpu)), gap: of(frameTimes.map((f) => f.gap)), gpu: null, brep: null };
  const [a, b] = [counted[0], counted.at(-1)];
  const d = (name, k = 0) => (b?.said[name]?.[k] ?? 0) - (a?.said[name]?.[k] ?? 0);
  if (a && b && d("gpu.frame") > 0) out.gpu = { frames: d("gpu.frame"), mean: d("gpu.frame", 1) / d("gpu.frame") / 1e6 };
  const n = d("brep.counted_frames");
  if (n > 0) out.brep = Object.fromEntries(["fragments", "intersections", "trim_loops", "trim_edges", "shaded"].map((k) => [k, d(`brep.${k}`) / n]));
  return out;
}
/** All of it: `{ project, reached: { name: ms }, engines: [{ engine, op, asked, answered, worked }], blocked: [{ at, for }] }`. */
export const times = () => ({ project: of, reached: { ...reachedAt }, engines: engines.map((e) => ({ ...e })), blocked: blocked.map((b) => ({ ...b })), log: log().map((r) => ({ ...r, at: Math.round(r.at), ms: Math.round(r.ms) })), frames: frames() });
globalThis.topoTimes = times;
