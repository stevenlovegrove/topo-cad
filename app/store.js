// Where a project's files live: its scripts (.ts), and what is kept beside a model of what was put
// in its scene (its generated files: see workspace.js). Every backend has the same shape:
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
//   BranchStore   a commit on a branch of a repository (repo.js: GitHub's, or the
//                 host's), which it stays at till told otherwise. Read-only itself:
//                 edits are kept over it, and committed from there (work.js).

import { OWN_ORIGINS } from "./config.js";
import { GitHub } from "./github.js";

// ----- a tiny IndexedDB key/value helper ---------------------------------------

export function idb(dbName, storeName) {
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
    /** Who is saving, where the site serves a workspace: a save is then their commit. */
    this.who = null;
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
  /** A file as a worker can read its byte ranges: `{ url }`, or `{ file }`. */
  async bytes(id) {
    return { url: new URL(this.url(id), location.href).href };
  }
  async list() {
    const r = await fetch(this.base + "manifest.json", { cache: "no-cache" });
    this.manifest = await r.json();
    return this.manifest.files.filter((f) => isSource(f.id));
  }
  async read(id) {
    // (What the site lists is what it has: a file it does not list, a model's sidecar that nobody has written, is not asked for.)
    if (this.manifest && !this.manifest.files.some((f) => f.id === id)) return null;
    const r = await fetch(this.url(id), { cache: "no-cache" });
    return r.ok ? r.text() : null;
  }
  async write(id, text) {
    if (!this.writable) throw new Error(`${this.label} are read-only here`);
    const r = await fetch(this.url(id) + (this.who ? `?who=${encodeURIComponent(this.who)}` : ""), { method: "PUT", body: text });
    if (!r.ok) throw new Error(`saving ${id}: ${r.status} ${await r.text()}`);
    if (this.manifest && !this.manifest.files.some((f) => f.id === id)) this.manifest.files.push({ id });
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
  /** A file as a worker can read its byte ranges: `{ url }`, or `{ file }`; null where it is not there. */
  async bytes(id) {
    try {
      return { file: await (await this.handle(id)).getFile() };
    } catch (_) {
      return null;
    }
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
  /** Every file kept here, whatever it is. */
  ids() {
    return this.db.keys();
  }
  remove(id) {
    return this.db.del(id);
  }
  /** What is kept of a file, whole; and that kept. */
  entry(id) {
    return this.db.get(id);
  }
  put(id, e) {
    return this.db.set(id, e);
  }
}

/** What is kept over a store that is nobody else's: what is made again (how the model was settled, or drawn), and your
 *  own layer (docs/layers.md). */
export const LOCAL_ONLY = /\.(solved|drawn)\.json$|\.user\.gen\.ts$/;

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
  bytes(id) {
    return this.base.bytes?.(id) ?? null;
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
  /** The files kept over the store that are someone's edits. */
  async edits() {
    return (await this.top.ids()).filter((id) => !LOCAL_ONLY.test(id));
  }
  /** Lets those go: the files are then as the store has them. */
  async discardEdits() {
    await Promise.all((await this.edits()).map((id) => this.top.remove(id)));
  }
}

// ----- a branch of a repository ---------------------------------------------------------

/** How long a head known to be behind (the one a commit here was made from) is not gone back to, if a read still says
 *  it: reads straight after a commit can be behind it (docs/github-store.md). */
const BEHIND_FOR = 30000;
/** What is never looked at for scripts. */
const NOT_SOURCES = /(^|\/)(node_modules|target|pkg|\.[^/]*)\//;

/** Whether the page is at an address it shares with others: every project's Pages site of an account is at
 *  `account.github.io/project`, one origin, each able to read what any other keeps in the browser. (Not where those sites
 *  are all this copy's owner's: `OWN_ORIGINS`.) */
export const sharedOrigin = (at = location) => /\.github\.io$/i.test(at.hostname) && !/^\/(index\.html)?$/.test(at.pathname) && !OWN_ORIGINS.includes(at.hostname.toLowerCase());

const TOKEN_KEY = "topo.github.token";
/** The branches followed here and the token for them (`{ token, repos }`, as `keepGithub` kept them). On a shared origin
 *  the token is only ever kept for this tab (it outlives loading the page, which going to another project does). */
export async function readGithub(shared = sharedOrigin()) {
  const kept = (await settings.get("github").catch(() => null)) ?? { token: null, repos: [] };
  if (!shared) return kept;
  let token = null;
  try { token = sessionStorage.getItem(TOKEN_KEY); } catch (_) { /* a browser that keeps nothing: asked for again */ }
  return { ...kept, token };
}
export async function keepGithub({ token, ...rest }, shared = sharedOrigin()) {
  await settings.set("github", { ...rest, token: shared ? null : token });
  if (!shared) return;
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch (_) { /* as above */ }
}
/** Where a token is made that can do no more than this asks of it: read one repository's contents, or write them, for a month. */
export const tokenLink = (owner, access = "read") => `https://github.com/settings/personal-access-tokens/new?name=topo-cad&description=${encodeURIComponent(`${access === "write" ? "Commits to" : "Reads"} the repository a topo-cad project is in`)}&expires_in=30&contents=${access}${owner ? `&target_name=${encodeURIComponent(owner)}` : ""}`;
/** Why a token is not taken, where it is not: only a fine-grained one can be held to one repository. */
export const tokenRefused = (token) => (token && !token.startsWith("github_pat_") ? "use a fine-grained token (it begins github_pat_): any other kind can reach everything its owner can" : null);

/** How far apart the numbers of things made where others are making them too: two at one moment should not meet. */
const FRESH_FROM = 10000, FRESH_TO = 99999;
/** A number for a new thing, of those `used`: the next, where this page is all that makes them; else one that others
 *  making things at the same moment, who cannot yet see this one, will not also take. (Should they, the merge says so.) */
export function freshNumber(used, shared) {
  if (!shared) return Math.max(0, ...used) + 1;
  for (;;) {
    const n = FRESH_FROM + Math.floor(Math.random() * (FRESH_TO - FRESH_FROM + 1));
    if (!used.includes(n)) return n;
  }
}

/** Blobs, trees and commits as read, by their shas: they never change, so the page has no need to ask for them twice. Not on
 *  an address shared with others, who could read them; and let go of on signing out. */
const gitObjects = idb("topo-cad-git-objects", "objects");
export const forgetGitObjects = () => new Promise((done) => {
  const r = indexedDB.deleteDatabase("topo-cad-git-objects");
  r.onsuccess = r.onerror = r.onblocked = () => done();
});

/** Where the commit a branch's page is at is kept. */
export const checkoutKey = (repo, branch) => `checkout:${repo}@${branch}`;

/** The place a location's name says: `github:owner/name@branch` (the page's address names it); else null. */
export function githubPlace(kind) {
  const m = /^github:([\w.-]+\/[\w.-]+)@([\w./-]+)$/.exec(kind ?? "");
  return m && { repo: m[1], branch: m[2] };
}

/** A branch of a repository on GitHub. */
export const githubBranch = ({ repo, branch, token = null, api, place = null }) => new BranchStore({ git: new GitHub({ repo, token, api, kept: sharedOrigin() ? null : gitObjects }), repo, branch, place });

/**
 * A commit of a branch, and its files by their blobs. It stays at that commit, here and when the page is opened again, till
 * told to move: looking at where the branch has got to moves nothing. Stamps are blob shas, so a page finds what has changed
 * when it does move by the same `stat` as any store.
 */
export class BranchStore {
  /** `git`: the repository (repo.js); `repo`: what it is called. `place`: what the commit it is at is kept under, where that is not
   *  the branch's own: a session's. */
  constructor({ git, repo, branch, place = null }) {
    this.git = git;
    /** Heads known to be behind, and when they were found so. */
    this.behind = new Map();
    this.repo = repo;
    this.branch = branch;
    this.label = `${repo} (${branch})`;
    this.writable = false;
    /** The commit the page is at, and its files: `{ commit, files: path → blob }`. */
    this.at = null;
    /** Where the branch is, as last looked at (null: not yet). */
    this.head = null;
    /** The commit the branch was left at to look at an old one, while that is so. */
    this.viewing = null;
    /** What each commit has, as read. */
    this.trees = new Map();
    /** Why the branch could not be looked at, the last time it was. */
    this.trouble = null;
    this.opening = null;
    this.listeners = [];
    this.key = place ?? checkoutKey(repo, branch);
  }
  /** `f()` when it has moved, or where the branch is has been found out. */
  onMove(f) {
    this.listeners.push(f);
  }
  said() {
    for (const f of this.listeners) f();
  }
  /** Where the branch is now. The page stays where it is. */
  async look() {
    try {
      const commit = await this.git.look(this.branch);
      this.trouble = null;
      if (Date.now() - (this.behind.get(commit) ?? -Infinity) >= BEHIND_FOR) this.head = commit;
    } catch (e) {
      this.trouble = String(e.message ?? e);
      throw e;
    } finally {
      this.said();
    }
    return this.head;
  }
  /** The tree a commit has, and its files: `{ tree, files }`. */
  treeOf(commit) {
    let t = this.trees.get(commit);
    if (!t) {
      t = this.git.files(commit);
      this.trees.set(commit, t);
      t.catch(() => this.trees.delete(commit));
    }
    return t;
  }
  /** The page at this commit, here and when it is next opened. */
  async moveTo(commit) {
    this.at = { commit, files: (await this.treeOf(commit)).files };
    await settings.set(this.key, { commit, viewing: this.viewing }).catch(() => {});
    this.said();
  }
  /** An old commit to look at. */
  async view(commit) {
    this.viewing ??= this.at.commit;
    await this.moveTo(commit);
  }
  async leaveView() {
    if (!this.viewing) return;
    const to = this.viewing;
    this.viewing = null;
    await this.moveTo(to);
  }
  /** A commit made from here is where the branch is now, and the page is at it: these files (`{ id, sha, text }`; no sha:
   *  gone) changed. (Reads straight after it may say otherwise: those it comes from are not gone back to.) */
  async committed(commit, changed) {
    this.behind.set(this.at.commit, Date.now());
    if (this.head) this.behind.set(this.head, Date.now());
    const files = new Map(this.at.files);
    for (const { id, sha } of changed) {
      if (sha) files.set(id, sha);
      else files.delete(id);
    }
    this.git.remember(commit, files, changed);
    this.head = commit;
    this.at = { commit, files };
    await settings.set(this.key, { commit, viewing: null }).catch(() => {});
    this.said();
  }
  /** The branch was moved here to a commit that was already made. */
  async committedAs(commit) {
    if (this.head) this.behind.set(this.head, Date.now());
    this.head = commit;
    await this.moveTo(commit);
  }
  /** What it has now. The first time: the commit it was left at, or where the branch is. */
  current() {
    if (this.at) return Promise.resolve(this.at);
    return (this.opening ??= this.open().finally(() => { this.opening = null; }));
  }
  async open() {
    const kept = await settings.get(this.key).catch(() => null);
    if (kept?.commit) {
      this.viewing = kept.viewing ?? null;
      try {
        await this.moveTo(kept.commit);
        return this.at;
      } catch (_) {
        // (No longer there: what the branch had was forced away.)
        this.viewing = null;
      }
    }
    try {
      await this.look();
    } catch (e) {
      throw this.git.openTrouble(e, this.branch);
    }
    await this.moveTo(this.head);
    return this.at;
  }
  async list() {
    const { files } = await this.current();
    return [...files].filter(([id]) => isSource(id) && !NOT_SOURCES.test(id)).map(([id, sha]) => ({ id, mtime: sha }));
  }
  async read(id) {
    const sha = (await this.current()).files.get(id);
    return sha ? this.git.blob(sha) : null;
  }
  /** A file as a worker can read its byte ranges, where the repository can say. */
  bytes(id) {
    return this.git.bytes(id);
  }
  async write() {
    throw new Error(`${this.label} is read-only here`);
  }
  async stat(ids) {
    const { files } = await this.current();
    return Object.fromEntries(ids.map((id) => [id, files.get(id) ?? null]));
  }
}
