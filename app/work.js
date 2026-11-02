// A project's versions, as git has them, whoever keeps them: what the git tool (git.js) and the Changes panel act on. Each kind
// says what it can do (`can`, of ACTIONS), and why not (`whyNot`), so that what cannot be done is neither offered nor tried:
//
//   BranchWork   a project on a branch of a repository (repo.js), for one person: below
//   TreeWork     a project in `topo serve`'s own files, where the working tree is the changes (host-git.js)
//   SessionWork  a branch as a session has it (session/work.js)
//
// All say the same `status()`, and take the same calls.
//
// A BranchWork is a commit the page is at, edits kept in this browser over it, and things done to them on purpose. Nothing here
// happens by itself: the page does not follow the branch, and edits stay edits till someone commits them.
//
// Pull, merge and revert are one thing: they bring changes into the edits, three ways (topo-sync), stopping at what clashes
// till each such file has been resolved. Then it is the edits that are committed, or looked at first. A merge remembers the
// branch it brings in, so that its commit has two parents: the first ours, the second theirs.
//
// git.js is these as a tool, for people and agents alike.

import { BrowserStore, checkoutKey, LOCAL_ONLY, OverlayStore, settings } from "./store.js";
import { Stale } from "./repo.js";

/** All that can be asked of a project's versions. */
export const ACTIONS = ["status", "fetch", "pull", "push", "diff", "commit", "restore", "branches", "branch", "switch", "merge", "log", "checkout", "revert", "conflicts", "resolve", "abort", "undo", "session", "share"];

/** A file's blob sha, as git has it: what it is stamped with whichever side has it, so a commit changes no stamp. */
async function blobSha(text) {
  const bytes = new TextEncoder().encode(text), head = new TextEncoder().encode(`blob ${bytes.length}\0`);
  const all = new Uint8Array(head.length + bytes.length);
  all.set(head);
  all.set(bytes, head.length);
  return [...new Uint8Array(await crypto.subtle.digest("SHA-1", all))].map((b) => b.toString(16).padStart(2, "0")).join("");
}

let merging = null;
/** Merging (topo-sync), once it is loaded. */
export const merger = () => (merging ??= import(new URL("../pkg/topo_sync.js", import.meta.url).href).then(async (m) => (await m.default(), m)));

/** What is said where something cannot be done. */
export class Cannot extends Error {}

const asObject = (files) => JSON.stringify(Object.fromEntries(files));
const short = (sha) => sha?.slice(0, 7) ?? "";

const listeners = new Set();
/** `f()` when any working copy has changed: what is edited, where it is, what is in conflict. */
export const onWork = (f) => listeners.add(f);
export const workChanged = () => { for (const f of listeners) f(); };
/** Two versions shown in the views: said by those (`showVersions`). */
export const versions = { show: null };

/** What a branch's edits are kept in, here. */
export const editsOf = (kind) => new BrowserStore(`topo-cad-edits:${kind}`);

/**
 * The branch with this browser's edits over it, kept for that branch. An edit is `{ text, mtime, sha }`, or `{ gone: true }`
 * for a file taken away.
 */
export class BranchWork extends OverlayStore {
  constructor(base, top) {
    super(base, top);
    this.serial = Promise.resolve();
    /** What is being merged in, till it is committed: `{ branch, tip, message }`. */
    this.merging = null;
    /** Files waiting to be settled: `{ doing, paths, ours, theirs, marked (each path → text; null: gone), chosen, writes, finish, optional: nothing
     *  else need wait for it }`. */
    this.clash = null;
    /** Saves of edits, to step back and forward through: `{ id, was, now }`. */
    this.undos = [];
    this.redos = [];
    this.key = `merging:${base.repo}@${base.branch}`;
    this.loaded = settings.get(this.key).then((m) => { this.merging = m ?? null; }).catch(() => {});
    base.onMove(() => this.said());
  }
  said() {
    workChanged();
  }
  /** Whether this can be asked of it. (Each commit goes on to the branch: nothing is left to push.) */
  can(action) {
    return ACTIONS.includes(action) && action !== "push" && action !== "share";
  }
  whyNot(action) {
    if (action === "share") return "a session on this branch is already open to anyone who can read the repository, with its link";
    return action === "push" ? "a commit here is pushed as it is made" : `there is no git action ${action}`;
  }
  /** Others may be making things in it, unseen by this page: things made take numbers two people will not both take. */
  get sharedWithOthers() {
    return true;
  }
  /** What a session begun from it is of. */
  get sessionFrom() {
    return { branch: this.base.branch, commit: this.base.at.commit, git: this.base.git, name: this.base.repo };
  }
  /** One change to the edits at a time. */
  queue(f) {
    const r = this.serial.then(f);
    this.serial = r.catch(() => {});
    return r;
  }

  // ----- as a store: the files, with the edits over them ---------------------------------------

  async entries() {
    const out = [];
    for (const id of await this.edits()) {
      const e = await this.top.entry(id);
      if (e) out.push({ id, ...e });
    }
    return out;
  }
  async list() {
    const [a, kept] = await Promise.all([this.base.list(), this.top.ids()]);
    const m = new Map(a.map((f) => [f.id, f]));
    for (const id of kept) {
      const e = await this.top.entry(id);
      if (e?.gone) m.delete(id);
      else if (id.endsWith(".ts") && !id.endsWith(".d.ts")) m.set(id, { id, mtime: e?.sha ?? e?.mtime });
    }
    return [...m.values()];
  }
  async read(id) {
    const e = await this.top.entry(id);
    return e ? (e.gone ? null : e.text) : this.base.read(id);
  }
  async stat(ids) {
    const branch = await this.base.stat(ids), out = {};
    for (const id of ids) {
      const e = await this.top.entry(id);
      out[id] = !e ? branch[id] : e.gone ? null : e.sha ?? e.mtime;
    }
    return out;
  }
  /** A file saved: kept as an edit, unless it is as the commit the page is at has it. */
  write(id, text) {
    if (LOCAL_ONLY.test(id)) return this.top.write(id, text);
    return this.queue(async () => {
      if (this.base.viewing) throw new Cannot(`HEAD is detached at ${short(this.base.at?.commit)}: switch to ${this.base.branch} to change anything`);
      const was = await this.read(id);
      await this.keep(id, text);
      if (was !== text) {
        this.undos.push({ id, was, now: text });
        this.redos = [];
      }
      this.said();
    });
  }
  /** Keeps a file as an edit, or as none where the commit has it so. `text` null: gone. */
  async keep(id, text) {
    await this.base.current();
    const had = this.base.at.files.get(id) ?? null;
    if (text == null) {
      if (had) await this.top.put(id, { gone: true, mtime: Date.now() });
      else await this.top.remove(id);
      return;
    }
    const sha = await blobSha(text);
    if (sha === had) await this.top.remove(id);
    else await this.top.put(id, { text, mtime: Date.now(), sha });
  }
  /** The edits made these alone (`{ path: text }`; null: gone). */
  async setEdits(writes) {
    await Promise.all((await this.edits()).map((id) => this.top.remove(id)));
    for (const [id, text] of Object.entries(writes)) await this.keep(id, text);
  }

  // ----- undo: of saves, not of commits ---------------------------------------------------------

  /** The last save taken back: whether there was one. */
  undo() {
    return this.step(this.undos, this.redos, "was");
  }
  redo() {
    return this.step(this.redos, this.undos, "now");
  }
  step(from, to, side) {
    return this.queue(async () => {
      const s = from.pop();
      if (!s) return false;
      await this.keep(s.id, s[side]);
      to.push(s);
      this.said();
      return true;
    });
  }

  // ----- where it is ----------------------------------------------------------------------------

  /** What there is to say of it. */
  async status() {
    await this.loaded;
    const { base } = this, edits = await this.entries();
    return {
      repo: base.repo,
      branch: base.branch,
      commit: base.at?.commit ?? null,
      head: base.head,
      moved: !!base.head && !base.viewing && base.head !== base.at?.commit,
      viewing: !!base.viewing,
      upstream: base.git.upstream(base.branch),
      pushes: base.git.pushes,
      can: ACTIONS.filter((a) => this.can(a)),
      edited: edits.map((e) => ({ path: e.id, gone: !!e.gone })),
      merging: this.merging && { branch: this.merging.branch, message: this.merging.message },
      conflicts: this.clash ? this.clash.paths.filter((p) => !(p in this.clash.chosen)) : [],
      doing: this.clash?.doing ?? null,
      inTheWay: !!this.clash && !this.clash.optional,
      trouble: base.trouble,
    };
  }
  /** Looks at where the branch is, and goes nowhere. */
  async fetch() {
    await this.base.current();
    await this.base.look();
    this.said();
    return this.base.head;
  }
  /** What is edited, as a unified diff; of one file, or all. */
  async diff(path) {
    const lib = await merger(), out = [];
    await this.base.current();
    for (const e of await this.entries()) {
      if (path && e.id !== path) continue;
      const blob = this.base.at.files.get(e.id);
      out.push(lib.unified(e.id, blob ? await this.base.git.blob(blob) : "", e.gone ? "" : e.text));
    }
    return out.join("");
  }
  /** Edits let go of (git restore): of these files, or all. What is being merged in goes too, where all are. */
  restore(paths) {
    return this.queue(async () => {
      const ids = paths ?? (await this.edits());
      await Promise.all(ids.map((id) => this.top.remove(id)));
      if (!paths) await this.setMerging(null);
      this.undos = this.undos.filter((u) => !ids.includes(u.id));
      this.redos = this.redos.filter((u) => !ids.includes(u.id));
      this.said();
    });
  }
  async setMerging(m) {
    this.merging = m;
    await settings.set(this.key, m);
  }
  /** Nothing may be in the way of what is about to be done. */
  async mustBeClear(what, { edits = true } = {}) {
    await this.loaded;
    if (this.base.viewing) throw new Cannot(`HEAD is detached: switch to ${this.base.branch} before you ${what}`);
    if (this.clash && !this.clash.optional) throw new Cannot(`resolve the conflicts first, or abort`);
    if (this.merging) throw new Cannot(`a merge of ${this.merging.branch} is in progress: commit to conclude it, or abort`);
    if (edits && (await this.edits()).length) throw new Cannot(`commit or restore your changes before you ${what}`);
  }

  // ----- bringing changes in --------------------------------------------------------------------

  /**
   * What `incoming` changed since `base`, over `onto`: each `Map` path → blob, `texts` the text of edits in `incoming`,
   * `ours` which side is this person's ("onto" or "incoming").
   * `{ writes: { path: text, or null: gone }, clash: { path: { onto, incoming, marked } } }`.
   */
  async combine({ base, incoming, onto, texts = {}, ours }) {
    const lib = await merger(), { git } = this.base, writes = {}, clash = {};
    const textOf = async (files, path) => (files === incoming && path in texts ? texts[path] : files.get(path) ? git.blob(files.get(path)) : null);
    const plan = JSON.parse(lib.plan_merge(asObject(base), asObject(incoming), asObject(onto)));
    for (const [path, step] of Object.entries(plan)) {
      if (step === "ours") { writes[path] = await textOf(incoming, path); continue; }
      const [was, one, other] = await Promise.all([textOf(base, path), textOf(incoming, path), textOf(onto, path)]);
      const merged = step === "merge" ? lib.merge_file(path, was ?? "", one, other) : null;
      if (merged != null) writes[path] = merged;
      else clash[path] = { onto: other, incoming: one, marked: one != null && other != null ? (ours === "onto" ? lib.merge_marked(path, was ?? "", other, one) : lib.merge_marked(path, was ?? "", one, other)) : one ?? other };
    }
    return { writes, clash };
  }
  /** Stops at what clashes, if anything does; else finishes. `ours`: which side of `combine` is this person's. */
  async settle({ doing, writes, clash, ours, finish }) {
    const paths = Object.keys(clash);
    if (!paths.length) return finish(writes, false);
    const side = (k) => Object.fromEntries(paths.map((p) => [p, clash[p][k]]));
    this.clash = {
      doing,
      paths,
      ours: side(ours),
      theirs: side(ours === "onto" ? "incoming" : "onto"),
      marked: side("marked"),
      chosen: {},
      writes,
      finish,
    };
    this.said();
    return { clashed: paths };
  }
  /** A file in conflict resolved: `{ side: "ours" | "theirs" }` or `{ text }`. All resolved, what was being done is finished. */
  async resolve(path, { side, text }) {
    const c = this.clash;
    if (!c?.paths.includes(path)) throw new Cannot(`${path} is not in conflict`);
    c.chosen[path] = text !== undefined ? text : side === "ours" ? c.ours[path] : c.theirs[path];
    this.said();
    if (c.paths.some((p) => !(p in c.chosen))) return { left: c.paths.filter((p) => !(p in c.chosen)) };
    this.clash = null;
    return c.finish({ ...c.writes, ...c.chosen }, true);
  }
  /** What was under way undone (--abort): as if it had not been begun. */
  abort() {
    return this.queue(async () => {
      if (this.clash) this.clash = null;
      else if (this.merging) {
        await Promise.all((await this.edits()).map((id) => this.top.remove(id)));
        await this.setMerging(null);
      } else throw new Cannot("no merge or pull is in progress");
      this.said();
    });
  }
  /** The files of the commit it is at, with the edits over them: the map, and the edits' text. */
  async withEdits() {
    await this.base.current();
    const files = new Map(this.base.at.files), texts = {};
    for (const e of await this.entries()) {
      if (e.gone) files.delete(e.id);
      else { files.set(e.id, e.sha); texts[e.id] = e.text; }
    }
    return { files, texts };
  }
  /** The page brought up to where the branch is, the edits merged with it (git pull --autostash). */
  async pull() {
    await this.mustBeClear("pull", { edits: false });
    const { base } = this, head = await this.fetch();
    if (head === base.at.commit) return { pulled: false };
    const edits = await this.edits();
    if (!edits.length) {
      await base.moveTo(head);
      return { pulled: true };
    }
    const { files: incoming, texts } = await this.withEdits();
    const { writes, clash } = await this.combine({ base: base.at.files, incoming, onto: (await base.treeOf(head)).files, texts, ours: "incoming" });
    return this.settle({
      doing: `pulling ${base.branch}`,
      writes,
      clash,
      ours: "incoming",
      finish: async (all) => {
        // (Edits are of the commit they are kept over: it moves first, so what is as it now has it is no edit.)
        await base.moveTo(head);
        await this.setEdits(all);
        this.undos = [];
        this.redos = [];
        this.said();
        return { pulled: true };
      },
    });
  }
  /** Where the branch is, the page moved there too if nothing is in the way: asked for by someone looking at what there is,
   *  with nothing open from here. */
  async fastForward() {
    await this.loaded;
    await this.base.current();
    const head = await this.base.look();
    if (head === this.base.at.commit || this.base.viewing || this.clash || this.merging || (await this.edits()).length) return false;
    await this.base.moveTo(head);
    return true;
  }
  /** Another branch merged into this one. What merges cleanly is committed; what clashes stops to be resolved, and a commit
   *  then concludes it. */
  async merge(branch, message) {
    await this.mustBeClear("merge");
    const { base } = this, { git } = base, head = await this.fetch();
    if (head !== base.at.commit) throw new Cannot(`${base.branch} is behind ${git.upstream(base.branch)}: pull first`);
    const tip = await git.head(branch).catch((e) => { throw e.status === 404 ? new Cannot(`no such branch: ${branch}`) : e; });
    const apart = await git.compare(head, tip);
    if (apart.behind === 0) return { merged: false, text: `already up to date with ${branch}` };
    const said = message || `Merge ${branch} into ${base.branch}`;
    if (apart.ahead === 0) {
      await git.moveBranch(base.branch, tip);
      await base.committedAs(tip);
      return { merged: true, text: `fast-forwarded ${base.branch} to ${branch}, ${short(tip)}` };
    }
    const { writes, clash } = await this.combine({ base: (await base.treeOf(apart.base)).files, incoming: (await base.treeOf(tip)).files, onto: base.at.files, ours: "onto" });
    return this.settle({
      doing: `merging ${branch}`,
      writes,
      clash,
      ours: "onto",
      finish: async (all, settled) => {
        await this.setMerging({ branch, tip, message: said });
        await this.setEdits(all);
        this.said();
        if (settled) return { merged: false, text: `resolved: commit to conclude the merge of ${branch}` };
        return { merged: true, text: `merged ${branch}: ${short(await this.commit(said))}` };
      },
    });
  }
  /** What a commit did taken back, as edits to commit. */
  async revert(commit) {
    await this.mustBeClear("revert");
    const { base } = this, { git } = base, c = await git.commitOf(commit);
    if (!c.parents.length) throw new Cannot("that is the root commit: it has no parent to revert to");
    const { writes, clash } = await this.combine({ base: (await base.treeOf(commit)).files, incoming: (await base.treeOf(c.parents[0])).files, onto: base.at.files, ours: "onto" });
    return this.settle({
      doing: `reverting ${short(commit)}`,
      writes,
      clash,
      ours: "onto",
      finish: async (all) => {
        await this.setEdits(all);
        this.said();
        return { reverted: true };
      },
    });
  }
  /** An old commit looked at, and nothing else moved. */
  async view(commit) {
    await this.loaded;
    if (this.clash || this.merging) throw new Cannot("resolve or abort what is in progress first");
    if ((await this.edits()).length) throw new Cannot("commit or restore your changes before you check out a commit");
    await this.base.current();
    await this.base.view(commit);
    this.said();
  }
  async leaveView() {
    await this.base.leaveView();
    this.said();
  }

  // ----- committing -----------------------------------------------------------------------------

  /** A commit of the edits, or of these files of them, on the branch: its sha. The branch must be where the page is: a
   *  commit never merges by itself. */
  async commit(message, paths) {
    await this.loaded;
    const { base } = this, { git } = base;
    if (!message?.trim()) throw new Cannot("a commit needs a message");
    if (base.viewing) throw new Cannot(`HEAD is detached: switch to ${base.branch} to commit`);
    if (this.clash && !this.clash.optional) throw new Cannot("resolve the conflicts first");
    if (this.merging && paths) throw new Cannot(`a merge of ${this.merging.branch} is in progress: a merge is committed whole`);
    const all = await this.entries(), sent = paths ? all.filter((e) => paths.includes(e.id)) : all;
    if (!sent.length && !this.merging) throw new Cannot(paths ? "none of those files has changes" : "no changes to commit");
    let head;
    try {
      head = await this.fetch();
    } catch (e) {
      throw new Cannot(`${git.name} could not be asked where the branch is: ${git.explain(e)}`);
    }
    const moved = () => new Cannot(`${base.branch} is behind ${git.upstream(base.branch)}: pull first, then commit`);
    if (head !== base.at.commit) throw moved();
    const changes = Object.fromEntries(sent.map((e) => [e.id, e.gone ? null : e.text]));
    let sha;
    try {
      if (this.merging) {
        const { sha: made } = await git.makeCommit([head, this.merging.tip], (await git.commitOf(head)).tree, changes, message);
        await git.moveBranch(base.branch, made);
        sha = made;
      } else {
        sha = await git.commit(base.branch, head, changes, message);
      }
    } catch (e) {
      if (e instanceof Stale) {
        await this.fetch().catch(() => {});
        throw moved();
      }
      throw new Cannot(git.explain(e));
    }
    await this.queue(async () => {
      for (const s of sent) {
        const e = await this.top.entry(s.id);
        if (e && e.sha === s.sha && !!e.gone === !!s.gone) await this.top.remove(s.id);
      }
      await base.committed(sha, sent.map((e) => ({ id: e.id, sha: e.gone ? null : e.sha, text: e.text })));
      await this.setMerging(null);
      this.undos = this.undos.filter((u) => !sent.some((s) => s.id === u.id));
      this.redos = this.redos.filter((u) => !sent.some((s) => s.id === u.id));
      this.said();
    });
    return sha;
  }

  // ----- the rest of what the tool asks ----------------------------------------------------------

  branches() {
    return this.base.git.branches();
  }
  /** The latest commits, from where it is looking. */
  async log(count = 30) {
    const { base } = this;
    return { commits: await base.git.log(base.viewing ?? base.head ?? base.at.commit, count), at: base.at.commit };
  }
  /** What is in conflict, each side's text. */
  conflicts() {
    const c = this.clash;
    return c && { doing: c.doing, files: Object.fromEntries(c.paths.filter((p) => !(p in c.chosen)).map((p) => [p, { ours: c.ours[p], theirs: c.theirs[p], marked: c.marked[p] }])) };
  }
  /** A new branch from here, its name good. What is edited here goes with it: `{ go }`, the location it is. */
  async newBranch(name) {
    const { base } = this, kind = base.git.kindOf(name);
    await this.mustBeClear("create a branch", { edits: false });
    await base.git.branch(name, base.at.commit);
    // (It begins where this is, and what is edited here goes with it.)
    await settings.set(checkoutKey(base.repo, name), { commit: base.at.commit });
    const there = editsOf(kind);
    for (const { id, ...kept } of await this.entries()) await there.put(id, kept);
    await this.restore();
    return { go: kind };
  }
  /** To a branch: `{ go }`, the location it is, where that is another page; `{ text }`, where it is here. */
  async switchTo(name) {
    const { base } = this;
    if (name !== base.branch) return { go: base.git.kindOf(name) };
    if (!base.viewing) return { text: `already on ${name}` };
    await this.leaveView();
    return { text: `on ${name}, at ${short(base.at.commit)}`, moved: true };
  }
}
