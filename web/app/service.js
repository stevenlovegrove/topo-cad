// The scan workspace service, if this site has one (`topo serve MODEL SCAN`).
// Without it the app is what it is on any static host: `probe()` is false
// and nothing else here is used.
//
// The service holds one workspace that every browser and agent works in.
// Its state is pushed (server-sent events); everything done goes through
// `call(tool, args)` as this browser's participant.

const ID_KEY = "topo.who";

function myId() {
  try {
    let id = localStorage.getItem(ID_KEY);
    if (!id) {
      id = "p-" + Math.random().toString(36).slice(2, 10);
      localStorage.setItem(ID_KEY, id);
    }
    return id;
  } catch (_) {
    return "p-" + Math.random().toString(36).slice(2, 10);
  }
}

/** A cloud as the service sends it (`topo_scan::cloud`). */
export function parseCloud(buf) {
  const n = new DataView(buf).getUint32(0, true);
  return {
    count: n,
    xyz: new Float32Array(buf, 4, 3 * n),
    intensity: new Uint8Array(buf, 4 + 12 * n, n),
    normal: new Int8Array(buf, 4 + 13 * n, 3 * n),
  };
}

export const service = {
  available: false,
  /** The model's id among the site's files. */
  entry: null,
  id: myId(),
  /** The workspace's state, as last pushed. */
  state: null,
  connected: false,
  listeners: [],

  async probe() {
    try {
      const r = await fetch("api/workspace", { cache: "no-cache" });
      if (!r.ok) return false;
      const j = await r.json();
      if (!j.service) return false;
      this.available = true;
      this.entry = j.entry;
      return true;
    } catch (_) {
      return false;
    }
  },

  /** Starts listening; `f(state)` on every change. */
  connect() {
    const es = new EventSource(`api/events?who=${encodeURIComponent(this.id)}`);
    es.addEventListener("state", (e) => {
      this.state = JSON.parse(e.data);
      this.connected = true;
      for (const f of this.listeners) f(this.state);
    });
    es.onerror = () => {
      this.connected = false;
      for (const f of this.listeners) f(this.state);
    };
  },
  onState(f) {
    this.listeners.push(f);
  },

  /** This browser's participant, once the service knows it. */
  me() {
    return this.state?.participants.find((p) => p.id === this.id) ?? null;
  },
  participant(id) {
    return this.state?.participants.find((p) => p.id === id) ?? { id, name: id, kind: "person", colour: "#888" };
  },

  /** Runs a tool: `{ text, images, error }`. */
  async call(tool, args = {}) {
    try {
      const r = await fetch("api/call", { method: "POST", body: JSON.stringify({ who: this.id, tool, args }) });
      const j = await r.json();
      if (j.error) return { error: j.error, text: "", images: [] };
      return {
        text: j.content.filter((c) => c.type === "text").map((c) => c.text).join("\n"),
        images: j.content.filter((c) => c.type === "image").map((c) => `data:${c.mimeType};base64,${c.data}`),
      };
    } catch (e) {
      return { error: String(e?.message ?? e), text: "", images: [] };
    }
  },

  /** Members with their evidence; with a proposal's commit, its before / after / changes / diff. */
  async members(proposal) {
    const r = await fetch("api/members" + (proposal ? `?proposal=${encodeURIComponent(proposal)}` : ""));
    return r.json();
  },
  async cloud() {
    const r = await fetch("api/cloud");
    if (!r.ok) return null;
    return parseCloud(await r.arrayBuffer());
  },
  /** Every point within `half` of the plane (scan frame), thinned to `max`. */
  async slab(origin, normal, half, max = 2000000) {
    const r = await fetch(`api/slab?o=${origin.join(",")}&n=${normal.join(",")}&half=${half}&max=${max}`);
    if (!r.ok) return null;
    return parseCloud(await r.arrayBuffer());
  },
  /** A selection's points (thinned), to draw. */
  async selectionCloud(id) {
    const r = await fetch(`api/selection?id=${encodeURIComponent(id)}`);
    if (!r.ok) return null;
    return parseCloud(await r.arrayBuffer());
  },
  /** What a ray meets (scan frame); `z` (model frame) and `slab` are the view's cuts. */
  async pick(origin, dir, z, slab) {
    const r = await fetch("api/pick", { method: "POST", body: JSON.stringify({ origin, dir, z, slab }) });
    return r.json();
  },
  /** Query for saving a file, so the save is a commit by this participant. */
  whoQuery() {
    return this.available ? `?who=${encodeURIComponent(this.id)}` : "";
  },
};
