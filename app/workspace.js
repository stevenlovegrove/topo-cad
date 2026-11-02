// The workspace of the project that is open: what has been put in its scene (slabs, polylines,
// views kept, rooms, planes), who is in it, what they have raised, and, where the project has
// a scan, the scan. The app works in it the same way wherever it is kept:
//
//   state               what there is, as last said; `onState(f)` hears of every change
//   call(tool, args)    does something in it (the tools are the ones an agent has): { text, images, error }
//   can(what)           whether this workspace has: "scan" (points: to draw, pick, select, fit to, and
//                       what they say of each member), "rays" (where each point was seen from),
//                       "shared" (other people and agents: issues, proposals, chat, readings),
//                       "people" (others in it, or in a session on it: their badges, and chat)
//
// It is kept by one of two backends, each of the shape below:
//
//   served   by `topo serve`, of a project that is a git repository of its own: one for every browser and agent, in
//            the project's git repository, with whatever needs the scan's points worked out there;
//   local    in the project's own files, wherever those are (store.js): the model's aids, in its generated
//            files (docs/layers.md), as a served one keeps them. This page's alone.
//
// A service keeps one for each project that can have one there, opened as it is asked for; any other project has a local one.

import { model, storage } from "./project.js";
import { freshNumber } from "./store.js";
import { git } from "./git.js";

// Who this browser is, in a workspace others are in: made once, and kept (as it is: not as JSON, as it always was).
const ID_KEY = "topo.who", NAME_KEY = "topo.session.name";
function myId() {
  const fresh = () => "p-" + Math.random().toString(36).slice(2, 10);
  try {
    let id = localStorage.getItem(ID_KEY);
    if (!id) localStorage.setItem(ID_KEY, (id = fresh()));
    return id;
  } catch (_) {
    return fresh();
  }
}

/** A cloud as the service sends it (`topo_scan::cloud`). */
function parseCloud(buf) {
  const n = new DataView(buf).getUint32(0, true);
  return {
    count: n,
    xyz: new Float32Array(buf, 4, 3 * n),
    intensity: new Uint8Array(buf, 4 + 12 * n, n),
    normal: new Int8Array(buf, 4 + 13 * n, 3 * n),
    // (A slab's points also say how much they are to be believed.)
    confidence: buf.byteLength >= 4 + 17 * n ? new Uint8Array(buf, 4 + 16 * n, n) : null,
  };
}

/** What every workspace's state has, whoever keeps it: nothing in it, till said otherwise. */
export const EMPTY = { loading: null, model: {}, scan: null, cloud: 0, placement: null, participants: [], issues: [], chat: [], views: [], rooms: [], surfaces: [], selections: [], slabs: [], polylines: [], shortcuts: [], tally: null };

/** What only a scan's points can answer, where there is none. */
const NO_SCAN = {
  evidence: async () => ({ parts: {} }),
  cloud: async () => null,
  slabPoints: async () => null,
  free: async () => null,
  selectionCloud: async () => null,
  selectionRays: async () => [],
  pick: async () => ({ hit: null }),
};

const failed = (error) => ({ error: String(error?.message ?? error), text: "", images: [] });
const boxQuery = (box) => ["c", "u", "v", "n", "h"].map((k) => `${k}=${box[k].join(",")}`).join("&");

// ----- served: one workspace for everyone, kept by the service --------------------------------

function served(id, said, entry) {
  // (Every request says which project it is of: the service keeps a workspace for each that is asked for.)
  const of = (path) => `${path}${path.includes("?") ? "&" : "?"}project=${encodeURIComponent(entry)}`;
  const get = (path) => fetch(of(path), { cache: "no-cache" });
  const post = (path, body) => fetch(of(path), { method: "POST", body: JSON.stringify(body) });
  const cloud = async (path) => { const r = await get(path); return r.ok ? parseCloud(await r.arrayBuffer()) : null; };
  return {
    shared: true,
    start() {
      const events = new EventSource(of(`api/events?who=${encodeURIComponent(id)}`));
      events.addEventListener("state", (e) => said({ ...EMPTY, ...JSON.parse(e.data) }, true));
      events.onerror = () => said(null, false);
    },
    async call(tool, args) {
      try {
        const j = await (await post("api/call", { who: id, tool, args })).json();
        if (j.error) return failed(j.error);
        return {
          text: j.content.filter((c) => c.type === "text").map((c) => c.text).join("\n"),
          images: j.content.filter((c) => c.type === "image").map((c) => `data:${c.mimeType};base64,${c.data}`),
        };
      } catch (e) {
        return failed(e);
      }
    },
    evidence: async () => (await get("api/evidence")).json(),
    diff: async (commit) => (await get(`api/diff?proposal=${encodeURIComponent(commit)}`)).json(),
    cloud: (k = 0) => cloud(`api/cloud?scan=${k}`),
    slabPoints: (box, max) => cloud(`api/slab?${boxQuery(box)}&max=${max}`),
    async free(box, w, h) {
      const r = await get(`api/free?${boxQuery(box)}&w=${w}&ht=${h}`);
      if (!r.ok) return null;
      const buf = await r.arrayBuffer(), d = new DataView(buf);
      return { w: d.getUint32(0, true), h: d.getUint32(4, true), max: d.getUint32(8, true), data: new Uint8Array(buf, 12) };
    },
    selectionCloud: (sel) => cloud(`api/selection?id=${encodeURIComponent(sel)}`),
    async selectionRays(sel) {
      const r = await get(`api/rays?id=${encodeURIComponent(sel)}`);
      return r.ok ? (await r.json()).rays ?? [] : [];
    },
    pick: async (origin, dir, z, slab, pixel) => (await post("api/pick", { origin, dir, z, slab, pixel })).json(),
  };
}

// ----- local: kept in the project's own files, for this page ---------------------------------

// What can be made or changed here, each an aid of the model (docs/layers.md): its tool, the list it is in, its ids'
// letter, what it is called in what the tool says back, and what of it can be set.
const KEPT = {
  slabs: { list: "slabs", letter: "b", word: "slab", set: ["centre", "axes", "half", "name"] },
  polylines: { list: "polylines", letter: "l", word: "polyline", set: ["points", "closed", "name"] },
  surfaces: { list: "surfaces", letter: "p", word: "plane", set: ["name"] },
};

function local(id, said, entry) {
  let doc = {};
  const say = () => said({ ...EMPTY, ...doc, scan: null, placement: null, participants: [{ id, name: "you", kind: "person", colour: "#5b8def", presence: "here" }] }, true);
  // (The model's aids, as it is built: again whenever it is.)
  const load = async () => { doc = (await model.aids()) ?? {}; say(); };
  model.onChange(load);
  // (These lists written, each whole, as aids of the model; it is built again, and they are read back from it.)
  const keep = async (...lists) => {
    const w = await model.writeAids(Object.fromEntries(lists.map((l) => [l, doc[l] ?? []])));
    if (w.error) throw new Error(w.error);
    await load();
  };
  const done = async (text, ...lists) => { await keep(...lists); return { text, images: [] }; };
  return {
    ...NO_SCAN,
    shared: false,
    start: load,
    async call(tool, args) {
      try {
        if (tool === "git") return await git(args);
        const action = args.action ?? "list";
        if (tool === "views") {
          const views = (doc.views ??= []), k = views.findIndex((v) => v.name === args.name);
          if (action === "save") { const v = { name: args.name, state: args.state }; k < 0 ? views.push(v) : (views[k] = v); return done(`saved the view ${args.name}`, "views"); }
          if (action === "delete" && k >= 0) { views.splice(k, 1); return done(`deleted the view ${args.name}`, "views"); }
          return failed(`no view ${args.name}`);
        }
        // Where things belong (alone: the project's, once named or kept on purpose; else scratch).
        if (tool === "objects") {
          const lists = Object.values(KEPT).map((k) => k.list);
          if (action === "clear") { for (const l of lists) doc[l] = (doc[l] ?? []).filter((x) => x.kept); return done("cleared what was scratch", ...lists); }
          const x = lists.flatMap((l) => doc[l] ?? []).find((x) => x.id === args.id);
          if (action !== "move" || !x || !["project", "scratch"].includes(args.to)) return failed(`nothing here is ${args.id}, or it cannot go to ${args.to}`);
          x.kept = args.to === "project";
          return done(`${x.id} is now ${args.to === "project" ? "the project's" : "scratch"}`, ...lists);
        }
        const kind = KEPT[tool];
        if (!kind) return failed("That needs a workspace kept by a service (topo serve, of a project that is a git repository of its own): this one is kept in the project's files, by this page alone.");
        const list = (doc[kind.list] ??= []);
        if (action === "create" && tool !== "surfaces") {
          const n = freshNumber(list.map((x) => Number(x.id.slice(1)) || 0), storage.sharedWithOthers());
          const made = { id: `${kind.letter}${n}`, name: "", by: id, ...Object.fromEntries(kind.set.filter((k) => args[k] != null).map((k) => [k, args[k]])) };
          made.kept = !!made.name;
          list.push(made);
          return done(`made ${kind.word} ${made.id}`, kind.list);
        }
        const x = list.find((x) => x.id === args.id);
        if (!x) return failed(`no ${kind.word} ${args.id}`);
        if (action === "delete") { list.splice(list.indexOf(x), 1); return done(`deleted ${args.id}`, kind.list); }
        // (A plane's name is its `label`, as its tool has it.)
        for (const k of kind.set) { const v = k === "name" ? args.name ?? args.label : args[k]; if (v != null) x[k] = v; }
        // (Named: worth keeping.)
        x.kept ||= !!x.name;
        return done(`${kind.word} ${x.id}`, kind.list);
      } catch (e) {
        return failed(e);
      }
    },
    diff: async () => ({ error: "Proposals are a served workspace's." }),
  };
}

// ----- the workspace ---------------------------------------------------------------------------

export const workspace = {
  id: myId(),
  /** What this person has said they are called, for what they make where it is kept by name; none: nothing said. */
  myName() {
    try { return localStorage.getItem(NAME_KEY); } catch (_) { return null; }
  },
  setMyName(name) {
    try { localStorage.setItem(NAME_KEY, name); } catch (_) { /* not kept here */ }
  },
  /** Whether this site keeps workspaces (for the projects there that can have one: asked of each as it is opened). */
  service: false,
  /** Its state, as last said (null: none is open yet). */
  state: null,
  connected: false,
  listeners: [],
  backend: null,
  entry: null,
  /** A session this page is in (session/enter.js): who is here and what is said come from it. `{ state(), call(tool, args) }`. */
  together: null,

  /** Whether this site keeps workspaces: asked once, on load. Saves there are then this participant's. */
  async probe() {
    try {
      const r = await fetch("api/workspace", { cache: "no-cache" });
      const j = r.ok ? await r.json() : null;
      if (!j?.service) return false;
      this.service = true;
      if (storage.site) storage.site.who = this.id;
      return true;
    } catch (_) {
      return false;
    }
  },

  /** The workspace of the project the page is of, once it is open: the service's where the service keeps one of it,
   *  else its own. (One, once: another project is another page.) */
  watch() {
    const follow = (s) => { if (s?.file && !this.entry) this.open(s.file); };
    model.onChange(follow);
    follow(model.state);
  },
  async open(entry) {
    this.entry = entry;
    const said = (state, connected) => {
      if (state) this.ownState = state;
      this.connected = connected;
      this.tell();
    };
    // (Asking opens it there: a project has a workspace on the service where it is a repository of its own.)
    // (Not a session's: what is in one is its own till it is committed, so what is put in its scene goes in its files, for everyone in it.)
    const kept = this.service && storage.kind === "host" && !storage.store.session && (await fetch(`api/workspace?project=${encodeURIComponent(entry)}`, { cache: "no-cache" }).then((r) => r.json()).catch(() => null))?.workspace;
    this.backend = kept ? served(this.id, said, entry) : local(this.id, said, entry);
    this.backend.start();
  },
  /** The state as it now is, said to everyone. */
  tell() {
    if (!this.ownState) return;
    this.state = this.together ? { ...this.ownState, ...this.together.state() } : this.ownState;
    for (const f of this.listeners) f(this.state);
  },
  /** What a session says has changed. */
  refresh() {
    this.tell();
  },
  /** `f(state)` on every change. */
  onState(f) {
    this.listeners.push(f);
  },

  can(what) {
    const s = this.state;
    if (what === "scan") return !!s?.scan;
    if (what === "rays") return !!s?.scan?.rays;
    if (what === "shared") return !!this.backend?.shared;
    if (what === "people") return !!this.backend?.shared || !!this.together;
    throw new Error(`workspace.can: ${what}?`);
  },

  /** This browser's participant, once the workspace knows it. */
  me() {
    return this.state?.participants.find((p) => p.id === this.id) ?? null;
  },
  participant(id) {
    return this.state?.participants.find((p) => p.id === id) ?? { id, name: id, kind: "person", colour: "#888" };
  },

  /** Runs a tool: `{ text, images, error }`. */
  call(tool, args = {}) {
    const theirs = this.together?.call(tool, args);
    if (theirs) return theirs;
    return this.backend ? this.backend.call(tool, args) : Promise.resolve(failed("no project is open"));
  },
  /** What the scan says of each member, by path: `{ parts: { path: { colour, evidence } } }`. */
  evidence() { return (this.backend ?? NO_SCAN).evidence(); },
  /** A proposal (by its commit) as before / after / changes / diff / offer. */
  diff(commit) { return this.backend ? this.backend.diff(commit) : Promise.resolve({ error: "no project is open" }); },
  /** The scan, thinned to draw. */
  cloud(k = 0) { return (this.backend ?? NO_SCAN).cloud(k); },
  /** Every point in a box ({ c, u, v, n, h }: centre, unit axes, half sizes; scan frame), thinned to `max`. */
  slabPoints(box, max = 2000000) { return (this.backend ?? NO_SCAN).slabPoints(box, max); },
  /** Free space in a box, over its first two axes: { w, h, max, data } (a byte per cell: how many rays pass over it, on a logarithmic scale; row 0 at the low side of the second axis). */
  free(box, w, h) { return (this.backend ?? NO_SCAN).free(box, w, h); },
  /** A selection's points (thinned), to draw. */
  selectionCloud(id) { return (this.backend ?? NO_SCAN).selectionCloud(id); },
  /** A selection's rays, sampled: [[sensor, point], …] (scan frame). */
  selectionRays(id) { return (this.backend ?? NO_SCAN).selectionRays(id); },
  /** What a ray meets of the scan (scan frame); `z` (model frame) and `slab` (a box, as above) are the view's cuts;
   *  `pixel` (a view's `pixel()`): met as near the cursor as the view shows it. */
  pick(origin, dir, z, slab, pixel) { return (this.backend ?? NO_SCAN).pick(origin, dir, z, slab, pixel); },
};
