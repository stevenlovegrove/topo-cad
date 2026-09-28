// The app's "server", in the page: answers the UI's `api(path, body)` calls
// (the same paths `topo serve` used to answer) from the engine worker and
// the chosen storage backend.
import { BrowserStore, FolderStore, HttpStore, OverlayStore, settings } from "./store.js";

// ----- the engine worker -------------------------------------------------------------

const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
let nextId = 1;
const pending = new Map();
worker.onmessage = (e) => {
  const { id, ok, value, error } = e.data;
  const p = pending.get(id);
  pending.delete(id);
  ok ? p.resolve(value) : p.reject(new Error(error));
};
worker.onerror = (e) => {
  for (const p of pending.values()) p.reject(new Error(`engine failed to start: ${e.message || "see the console"}`));
  pending.clear();
};
function engine(op, args) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, op, args });
  });
}

// ----- storage --------------------------------------------------------------------------

export const storage = {
  store: null,
  site: null, // the HttpStore, if this site has files
  folderWaiting: null, // a remembered folder awaiting permission
  listeners: [],
  onChange(f) {
    this.listeners.push(f);
  },
  async use(store, kind) {
    this.store = store;
    project = null;
    await settings.set("store", kind).catch(() => {});
    for (const f of this.listeners) f();
  },
  /** The storage choices this browser offers. */
  choices() {
    const out = [];
    if (this.site) out.push({ kind: "site", label: this.site.writable ? `${this.site.label}` : `${this.site.label} (read-only; edits kept in this browser)` });
    if (FolderStore.supported()) out.push({ kind: "folder", label: this.folderWaiting ? `Reconnect folder “${this.folderWaiting.dir.name}”` : "A folder on this computer…" });
    out.push({ kind: "browser", label: "Projects in this browser" });
    return out;
  },
  /** Switches storage (a folder needs a click: call from a user gesture). */
  async choose(kind) {
    if (kind === "site") return this.use(this.siteStore(), "site");
    if (kind === "browser") return this.use(new BrowserStore(), "browser");
    if (kind === "folder") {
      let f = this.folderWaiting;
      if (f && !(await f.permitted(true))) f = null;
      if (!f) f = await FolderStore.pick();
      this.folderWaiting = null;
      return this.use(f, "folder");
    }
  },
  siteStore() {
    return this.site.writable ? this.site : new OverlayStore(this.site, new BrowserStore("topo-cad-edits"));
  },
  async boot() {
    this.site = await HttpStore.detect();
    const saved = await settings.get("store").catch(() => null);
    if (saved === "folder" && FolderStore.supported()) {
      const f = await FolderStore.remembered().catch(() => null);
      if (f && (await f.permitted(false))) return this.use(f, "folder");
      this.folderWaiting = f; // needs a click to re-grant
    }
    if (saved === "browser" || !this.site) return this.use(new BrowserStore(), "browser");
    return this.use(this.siteStore(), "site");
  },
};

// ----- the open project -------------------------------------------------------------

let project = null; // { entry, ids, sidecar, stamps, version }
let lastCheck = 0;

/** The model file and every local file it imports, plus its sidecar if present. */
async function gather(entry) {
  const st = storage.store;
  const files = {};
  const todo = [entry];
  while (todo.length) {
    const id = todo.pop();
    if (id in files) continue;
    const text = await st.read(id);
    if (text == null) throw new Error(`cannot read ${id}`);
    files[id] = text;
    for (const dep of await engine("imports", { source: text, id }).catch(() => [])) todo.push(dep);
  }
  const sidecar = await engine("sidecarId", { id: entry });
  if (!(sidecar in files)) {
    const t = await st.read(sidecar);
    if (t != null) files[sidecar] = t;
  }
  return { files, sidecar };
}

async function open(entry) {
  const { files, sidecar } = await gather(entry);
  const version = await engine("open", { entry, files });
  const ids = Object.keys(files);
  project = { entry, ids, sidecar, stamps: await storage.store.stat([...new Set([...ids, sidecar])]), version };
  await settings.set(`last:${storage.store.label}`, entry).catch(() => {});
  return version;
}

/** Reloads the project if any of its files changed in storage (at most once a second). */
async function refresh(force = false) {
  if (!project) return;
  const now = Date.now();
  if (!force && now - lastCheck < 900) return;
  lastCheck = now;
  const ids = [...new Set([...project.ids, project.sidecar])];
  const stamps = await storage.store.stat(ids);
  if (!force && ids.every((id) => stamps[id] === project.stamps[id])) return;
  const { files } = await gather(project.entry);
  project.ids = Object.keys(files);
  project.stamps = await storage.store.stat([...new Set([...project.ids, project.sidecar])]);
  project.version = await engine("setFiles", { files });
}

let projectCache = new Map(); // id → { mtime, isModel }

async function projects() {
  const list = await storage.store.list();
  const out = [];
  for (const f of list) {
    if (f.id.endsWith(".measured.ts")) continue;
    let c = projectCache.get(f.id);
    if (!c || c.mtime !== f.mtime) {
      const isModel = f.project ?? /export\s+default/.test((await storage.store.read(f.id)) ?? "");
      c = { mtime: f.mtime, isModel };
      projectCache.set(f.id, c);
    }
    if (c.isModel) {
      const i = f.id.lastIndexOf("/");
      out.push({ group: i < 0 ? "" : f.id.slice(0, i), name: f.id.slice(i + 1).replace(/\.ts$/, ""), path: f.id });
    }
  }
  out.sort((a, b) => a.group.localeCompare(b.group) || a.name.localeCompare(b.name));
  return out;
}

async function ensureOpen() {
  if (project || !storage.store) return;
  const list = await projects();
  if (!list.length) return;
  const last = await settings.get(`last:${storage.store.label}`).catch(() => null);
  await open(list.some((p) => p.path === last) ? last : list[0].path);
}

// ----- the API ----------------------------------------------------------------------------

const q = (path) => Object.fromEntries(new URLSearchParams(path.split("?")[1] || ""));
const tail = (path, prefix) => Number(path.slice(prefix.length).split("?")[0]);

export async function api(path, body) {
  try {
    return await route(path, body);
  } catch (e) {
    return { error: String(e?.message ?? e) };
  }
}

async function route(path, body) {
  if (path === "/api/state") {
    await ensureOpen();
    if (!project) return { version: -1, file: "", error: storage.store ? "No project here yet: create one with the project menu (New project…)." : "Choose where your projects are stored." };
    await refresh();
    return engine("state");
  }
  if (path === "/api/projects") await ensureOpen();
  if (path === "/api/projects") return { current: project?.entry ?? "", projects: await projects(), storage: storage.store?.label, writable: storage.store?.writable };
  if (path === "/api/open") return { version: await open(body.path) };
  if (path === "/api/new") {
    const name = (body.path || "").trim();
    if (!name) throw new Error("give the project a name");
    const { slug, text } = await engine("newProject", { name });
    const id = `projects/${slug}.ts`;
    if ((await storage.store.read(id)) != null) throw new Error(`${id} already exists`);
    await storage.store.write(id, text);
    return { version: await open(id), file: id };
  }
  if (!project) throw new Error("no project open");
  if (path === "/api/sources") {
    const ids = [...project.ids];
    const files = [];
    for (const id of ids) files.push({ path: id, name: id, text: (await storage.store.read(id)) ?? "" });
    return { files };
  }
  if (path === "/api/source") {
    if (!project.ids.includes(body.path)) throw new Error(`${body.path} is not one of this model's files`);
    await storage.store.write(body.path, body.text);
    await refresh(true);
    const s = await engine("state");
    return { saved: body.path, version: s.version, model_error: s.error ?? null };
  }
  if (path === "/api/scenario") return { version: await engine("scenario", { name: body.path }) };
  if (path === "/api/exclude") return { version: await engine("exclude", { names: body }) };
  if (path === "/api/measure") {
    const w = await engine("measure", body);
    await storage.store.write(w.id, w.text);
    await refresh(true);
    return { line: w.line, file: w.id };
  }
  if (path.startsWith("/api/sheet/")) return engine("request", { op: "sheet", args: { sheet: tail(path, "/api/sheet/") } });
  if (path.startsWith("/api/overlay/")) return engine("request", { op: "overlay", args: { sheet: tail(path, "/api/overlay/") } });
  if (path.startsWith("/api/heat/")) return engine("request", { op: "heat", args: { sheet: tail(path, "/api/heat/"), ...q(path) } });
  if (path.startsWith("/api/member")) return engine("request", { op: "member", args: q(path) });
  if (path.startsWith("/api/ratios")) return engine("request", { op: "ratios", args: q(path) });
  if (path === "/api/mesh") return engine("request", { op: "mesh" });
  if (path.startsWith("/api/view3d")) {
    const a = q(path);
    const t0 = performance.now();
    const r = await engine("request", { op: "view3d", args: { az: Number(a.az), el: Number(a.el) } });
    return { ...r, ms: performance.now() - t0 };
  }
  for (const op of ["pick", "preview", "parse"]) if (path === `/api/${op}`) return engine("request", { op, args: body });
  throw new Error(`unknown request ${path}`);
}
