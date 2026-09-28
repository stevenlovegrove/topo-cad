// Where a project's files live. Every backend has the same shape:
//
//   list()        → [{ id, mtime }]   all .ts files (ids are "/"-separated paths)
//   read(id)      → text, or null if missing
//   write(id, t)  → stores the text (throws if read-only)
//   stat(ids)     → { id: mtime | null }   cheap change detection
//   label, writable
//
// Backends:
//   HttpStore     files beside the app on any static web server, listed in
//                 files/manifest.json. Read-only there; writable when served
//                 by `topo serve` (which accepts PUT).
//   FolderStore   a folder on this computer (File System Access API: Chrome,
//                 Edge). Edits land in your real files; outside edits are seen.
//   BrowserStore  files kept in this browser (IndexedDB).
//   OverlayStore  a read-only store with edits kept in another (e.g. a
//                 published project, changed locally in the browser).

// ----- a tiny IndexedDB key/value helper ---------------------------------------

function idb(dbName, storeName) {
  const open = () =>
    new Promise((resolve, reject) => {
      const r = indexedDB.open(dbName, 1);
      r.onupgradeneeded = () => r.result.createObjectStore(storeName);
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  const tx = async (mode, f) => {
    const db = await open();
    return new Promise((resolve, reject) => {
      const t = db.transaction(storeName, mode);
      const req = f(t.objectStore(storeName));
      t.oncomplete = () => resolve(req?.result);
      t.onerror = () => reject(t.error);
    });
  };
  return {
    get: (k) => tx("readonly", (s) => s.get(k)),
    set: (k, v) => tx("readwrite", (s) => s.put(v, k)),
    del: (k) => tx("readwrite", (s) => s.delete(k)),
    keys: () => tx("readonly", (s) => s.getAllKeys()),
    all: () => tx("readonly", (s) => s.getAll()),
  };
}

/** Settings that outlive a page load (chosen storage, folder handle). */
export const settings = idb("topo-cad-settings", "kv");

const isSource = (id) => id.endsWith(".ts") && !id.endsWith(".d.ts");

// ----- files on the web server ---------------------------------------------------

export class HttpStore {
  constructor(base = "files/") {
    this.base = base;
    this.label = "Site files";
    this.writable = false;
    this.manifest = null;
  }
  /** The store if the site has a manifest, else null. */
  static async detect(base = "files/") {
    const s = new HttpStore(base);
    try {
      const r = await fetch(base + "manifest.json", { cache: "no-cache" });
      if (!r.ok) return null;
      s.manifest = await r.json();
      s.writable = !!s.manifest.writable;
      if (s.manifest.label) s.label = s.manifest.label;
      return s;
    } catch (_) {
      return null;
    }
  }
  url(id) {
    return this.base + id.split("/").map(encodeURIComponent).join("/");
  }
  async list() {
    const r = await fetch(this.base + "manifest.json", { cache: "no-cache" });
    this.manifest = await r.json();
    return this.manifest.files.filter((f) => isSource(f.id));
  }
  async read(id) {
    const r = await fetch(this.url(id), { cache: "no-cache" });
    return r.ok ? r.text() : null;
  }
  async write(id, text) {
    if (!this.writable) throw new Error(`${this.label} are read-only here`);
    const r = await fetch(this.url(id), { method: "PUT", body: text });
    if (!r.ok) throw new Error(`saving ${id}: ${r.status} ${await r.text()}`);
  }
  /** From the manifest (one request; `topo serve` lists each file's ETag). */
  async stat(ids) {
    const files = await this.list();
    const m = new Map(files.map((f) => [f.id, f.mtime ?? true]));
    return Object.fromEntries(ids.map((id) => [id, m.get(id) ?? null]));
  }
}

// ----- a folder on this computer ---------------------------------------------------

export class FolderStore {
  constructor(dir) {
    this.dir = dir;
    this.label = `Folder “${dir.name}”`;
    this.writable = true;
  }
  static supported() {
    return typeof window !== "undefined" && "showDirectoryPicker" in window;
  }
  /** Asks the user for a folder (needs a click). */
  static async pick() {
    const dir = await window.showDirectoryPicker({ id: "topo-cad", mode: "readwrite" });
    await settings.set("folder", dir);
    return new FolderStore(dir);
  }
  /** The folder used last time, if the browser still allows it (may need a click to re-grant). */
  static async remembered() {
    const dir = await settings.get("folder").catch(() => null);
    return dir ? new FolderStore(dir) : null;
  }
  async permitted(ask = false) {
    const opts = { mode: "readwrite" };
    if ((await this.dir.queryPermission(opts)) === "granted") return true;
    return ask && (await this.dir.requestPermission(opts)) === "granted";
  }
  async handle(id, create = false) {
    const parts = id.split("/");
    let d = this.dir;
    for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p, { create });
    return d.getFileHandle(parts[parts.length - 1], { create });
  }
  async list() {
    const out = [];
    const walk = async (d, prefix, depth) => {
      for await (const [name, h] of d.entries()) {
        if (h.kind === "directory") {
          if (depth < 4 && !name.startsWith(".") && !["node_modules", "target", "pkg"].includes(name)) await walk(h, prefix + name + "/", depth + 1);
        } else if (isSource(name)) {
          out.push({ id: prefix + name, mtime: (await h.getFile()).lastModified });
        }
      }
    };
    await walk(this.dir, "", 0);
    return out;
  }
  async read(id) {
    try {
      return await (await (await this.handle(id)).getFile()).text();
    } catch (_) {
      return null;
    }
  }
  async write(id, text) {
    const w = await (await this.handle(id, true)).createWritable();
    await w.write(text);
    await w.close();
  }
  async stat(ids) {
    const out = {};
    for (const id of ids) {
      try {
        out[id] = (await (await this.handle(id)).getFile()).lastModified;
      } catch (_) {
        out[id] = null;
      }
    }
    return out;
  }
}

// ----- files kept in this browser -----------------------------------------------------

export class BrowserStore {
  constructor(name = "topo-cad-files") {
    this.db = idb(name, "files");
    this.label = "This browser";
    this.writable = true;
  }
  async list() {
    const keys = await this.db.keys();
    const all = await this.db.all();
    return keys.map((id, i) => ({ id, mtime: all[i].mtime })).filter((f) => isSource(f.id));
  }
  async read(id) {
    const v = await this.db.get(id);
    return v ? v.text : null;
  }
  async write(id, text) {
    await this.db.set(id, { text, mtime: Date.now() });
  }
  async stat(ids) {
    const out = {};
    for (const id of ids) out[id] = (await this.db.get(id))?.mtime ?? null;
    return out;
  }
}

// ----- a read-only store with local edits ---------------------------------------------

export class OverlayStore {
  constructor(base, top) {
    this.base = base;
    this.top = top;
    this.label = `${base.label} (edits kept in this browser)`;
    this.writable = true;
  }
  async list() {
    const [a, b] = await Promise.all([this.base.list(), this.top.list()]);
    const m = new Map(a.map((f) => [f.id, f]));
    for (const f of b) m.set(f.id, f);
    return [...m.values()];
  }
  async read(id) {
    return (await this.top.read(id)) ?? this.base.read(id);
  }
  write(id, text) {
    return this.top.write(id, text);
  }
  async stat(ids) {
    const [a, b] = await Promise.all([this.base.stat(ids), this.top.stat(ids)]);
    const out = {};
    for (const id of ids) out[id] = b[id] ?? a[id];
    return out;
  }
}
