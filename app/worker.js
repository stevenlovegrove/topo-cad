// The engine, off the UI thread: the topo-cad wasm module (settling part scripts, analysis, drawings, 3D) plus the
// browser's own JavaScript engine running the part scripts. Messages: {id, op, args} → {id, ok, value | error, ms}.
//
// A page has three of these (project.js): two that the model is asked of in turn (one answers
// while the other builds what the model has been changed to), and one that draws its sheets, so
// that neither a settle nor a drawing of some seconds holds up anything that is asked of the model.
// All are told of every change to what is built; one told of a change as `lazy` builds only when
// it is next asked for something or told of a change that is not, so that of several changes it
// builds the last.
import init, * as wasm from "../pkg/topo_wasm.js";

// Each step of its work said to the page as it begins (its build log), timed from when the epoch began (the page's
// clock and a worker's start apart): one the engine says (`__topo_step`, of what it is doing within something asked of
// it), each thing asked of it, and its stopping.
const now = () => performance.timeOrigin + performance.now();
const say = (step) => self.postMessage({ step, t: now() });
say("starting the engine");
globalThis.__topo_step = (what) => say(what);
const ready = (async () => {
  await init({ module_or_path: new URL("../pkg/topo_wasm_bg.wasm", import.meta.url) });
  // A native helper scripts call: a data table by its name.
  globalThis.__topo_data = (name) => wasm.table_json(name);
  // What a part script's scope reads, while the engine runs it (see topo-part).
  globalThis.__topo_value = (expr) => wasm.topo_value(expr);
  globalThis.__topo_geometry = (json) => wasm.topo_geometry(json);
  globalThis.__topo_said = (address) => wasm.topo_said(address);
})();

let app = null;
// One version counter across projects, so the UI sees every rebuild as new. (The engine that draws is told
// which build each change is, by the one the model is asked of: `version`.)
let generation = 0;
let stale = false;
// (A model settled elsewhere, to be taken as it is when something is first asked of it: `adopt`.)
let adopted = null;
const build = () => { app.rebuild(); stale = false; };
/** A change was made to what is built; `later`: it is built when something is next asked of it, not now. */
const rebuilt = (version, later) => {
  if (later) stale = true;
  else build();
  return (generation = version ?? generation + 1);
};
/** The model as it is now: built, if a change has been left unbuilt. */
const current = () => {
  if (adopted) { app?.free(); app = wasm.App.adopt(adopted.settled, adopted.build); adopted = null; }
  if (stale) build();
  return app;
};

const ops = {
  // (Each change, `lazy`: it is built when something is next asked of this engine, not now.)
  open({ entry, files, version, lazy }) {
    adopted = null;
    app?.free();
    app = new wasm.App(entry, JSON.stringify(files));
    return rebuilt(version, lazy);
  },
  /** Which build of the engine this is (what a model it settled is kept by). */
  stamp() {
    return wasm.engine_stamp();
  },
  /** The model it has (built, or taken as settled elsewhere) said to be of another build than it was told: one built
   *  again as it was, which settled otherwise. */
  renumber({ version }) {
    return (generation = version);
  },
  /** The model as last settled, as data: what an engine that solves nothing makes its sheets or checks from. */
  settledOut() {
    return current().settled_out();
  },
  /** A model settled by another engine (`settledOut`'s), taken as it is when it is next asked for something: nothing
   *  is run or solved. `build`: what its sheets are drawn from is made of it; else only what its checks need. */
  adopt({ settled, version, build }) {
    app?.free();
    app = null;
    stale = false;
    adopted = { settled, build };
    generation = version;
    return generation;
  },
  setFiles({ files, version, lazy }) {
    app.set_files(JSON.stringify(files));
    return rebuilt(version, lazy);
  },
  scenario({ name, version, lazy }) {
    app.set_scenario(name || "");
    return rebuilt(version, lazy);
  },
  exclude({ names, version, lazy }) {
    app.set_excluded(JSON.stringify(names));
    return rebuilt(version, lazy);
  },
  state() {
    return app || adopted ? { ...JSON.parse(current().state()), version: generation } : null;
  },
  /** Which build its state would be of (none, where nothing is open): to look at its state only where that is new. */
  version() {
    return app || adopted ? generation : null;
  },
  request({ op, args }) {
    const out = current().request(op, JSON.stringify(args ?? {}));
    return op === "sheet" ? out : JSON.parse(out);
  },
  /** A plans script beside the model (`file`, its `text`), evaluated beside it: its tree, each leaf with the parts it makes. */
  plans({ file, text }) {
    return JSON.parse(current().plans(file, text));
  },
  /** Its sheets, drawn (the list of them, and what is amiss with them), and which build they are of: from what was kept
   *  of them as last drawn (`kept`: `NAME.drawn.json`'s text) where that is of this build; else drawn, and what to keep
   *  of them (`keep`: `{id, text}`, to write). */
  drawn({ kept } = {}) {
    if (kept != null) current().keep_drawing(kept);
    const drawn = JSON.parse(current().request("draw", "{}")), keep = current().part_drawn();
    return { ...drawn, version: generation, keep: keep ? JSON.parse(keep) : null };
  },
  /** The parts of another model, built for this and let go of: the open project's with other files (a proposal's side);
   *  with its bodies. */
  meshOf({ entry, files }) {
    const other = new wasm.App(entry, JSON.stringify(files));
    try {
      other.rebuild();
      const error = JSON.parse(other.state()).error;
      if (error) throw new Error(error);
      return { ...JSON.parse(other.request("mesh", "{}")), bodies: JSON.parse(other.request("bodies", "{}")) };
    } finally {
      other.free();
    }
  },
  /** Of a part script: params kept beside it (`{address: {value, tier}}`; null lets one go): `{id, text}`, to write. */
  partParams({ params }) {
    return JSON.parse(current().part_params(JSON.stringify(params)));
  },
  /** Of a part script: entries of your layer (by their keys; none: all) kept for everyone, moved into the model's own
   *  generated file: `[{id, text}]`, to write. */
  partShare({ keys }) {
    return JSON.parse(current().part_share(JSON.stringify(keys ?? null)));
  },
  /** Of a part script as built: its aids as the workspace has them, `{slabs, polylines, surfaces, selections, rooms,
   *  views}`. */
  partAids() {
    return JSON.parse(current().part_aids());
  },
  /** Of a part script: these lists of its aids written, each whole: `[{id, text}]`, to write. */
  partAidsWrite({ lists }) {
    return JSON.parse(current().part_aids_write(JSON.stringify(lists)));
  },
  /** Of a part script: the drags kept beside it, all of them: `{id, text}`, to write. */
  partDrags({ drags }) {
    return JSON.parse(current().part_drags(JSON.stringify(drags)));
  },
  /** Of a part script, as a person drags: what is grabbed (a node, a point of a part, a face) on the cursor's ray,
   *  `from` toward `to` (mm, in the root's frame), at the level dragged, with what is kept (in place of a drag kept of
   *  the same thing, or `beside` it): `{bodies, params}` as they then are. Nothing is written. */
  partGrab({ what, from, to, beside }) {
    return JSON.parse(current().part_grab(JSON.stringify(what), new Float64Array(from), new Float64Array(to), !!beside));
  },
  /** Of a part script as built: how a node can move with all that must hold held: `{rank, dirs, params}`. */
  partWays({ address }) {
    return JSON.parse(current().part_ways(address));
  },
  /** Of a part script: a drag let go of (the next begins from as built); kept (`keep`): `{id, text, drags}` to write
   *  (`drags`: those kept before it), or null where nothing was dragged. */
  partGrabEnd({ keep }) {
    const t = current().part_grab_end(!!keep);
    return t ? JSON.parse(t) : null;
  },
  /** Of a part script: a reading kept beside it by `name` (`{what, within}`), or the one of that name let go (none):
   *  `{id, text}`, to write. */
  partReading({ name, reading }) {
    return JSON.parse(current().part_reading(name, reading == null ? undefined : JSON.stringify(reading)));
  },
  /** Of a part script, built: how it was settled, to keep beside it (`{id, text}`), where that is new; else null. */
  partSolved() {
    const t = current().part_solved();
    return t ? JSON.parse(t) : null;
  },
  /** Of a part script, built: what checking it finds wrong, `{ key, said, version }` (`said` null: said with the
   *  model already, kept of its settle); null where it is not a part script. */
  partChecks() {
    const t = current().part_checks();
    return t ? { ...JSON.parse(t), version: generation } : null;
  },
  /** Of a part script: what checking the settle of `key` found, kept with that settle where it is the one built. */
  partChecked({ key, said }) {
    current().part_checked(key, JSON.stringify(said));
  },
  imports({ source, id }) {
    return JSON.parse(wasm.module_imports(source, id));
  },
  newProject({ name }) {
    return JSON.parse(wasm.new_project(name));
  },
};
// (One at a time, in the order asked.)
let last = Promise.resolve();
/** What each thing asked of it is, said in its log. */
const ASKED = {
  open: "opening the model", setFiles: "building it again", scenario: "building another scenario", exclude: "building it without some load cases",
  state: "saying how it is", settledOut: "handing out the settled model", adopt: "taking a settled model", drawn: "drawing its sheets", partChecks: "checking where its bodies meet", imports: "reading a file's imports",
};
let asking = 0;
self.onmessage = (e) => {
  const { id, op, args } = e.data;
  asking++;
  last = last.then(async () => {
    const began = performance.now();
    // (What it is asked once a second, to see whether its state is new, is not a step of its work.)
    if (op !== "version") say(ASKED[op] ?? (op === "request" ? `asked: ${args?.op}` : `asked: ${op}`));
    try {
      await ready;
      if (!ops[op]) throw new Error(`unknown engine op ${op}`);
      if (!app && !adopted && !["open", "adopt", "stamp", "state", "version", "imports", "newProject"].includes(op)) throw new Error("no model open");
      const value = ops[op](args ?? {});
      // (Bytes handed over, not copied.)
      self.postMessage({ id, ok: true, value, ms: performance.now() - began }, value instanceof Uint8Array ? [value.buffer] : []);
    } catch (err) {
      self.postMessage({ id, ok: false, error: String(err?.message ?? err), ms: performance.now() - began });
    }
    // (Nothing more asked of it: it stops.)
    if (--asking === 0) say(null);
  });
};
