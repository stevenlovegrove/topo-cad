// A project's versions on the `topo serve` the page is served by (crates/topo-cli/src/versions.rs): the git repository its folder is in.
// Each way of having them is over the same one request:
//
//   HostRepo   its objects, as a repository (repo.js): what a session is kept over, leaving the host's files alone till it commits
//   GuestRepo  the same from anywhere else, by the session's room on the relay (crates/topo-cli/src/share.rs): to read, and no more
//   TreeWork   its working tree, which is everyone's here: the changes are the files as they are on the host, and git is the host's

import { Refused, Repo, Stale } from "./repo.js";
import { ACTIONS, Cannot, workChanged } from "./work.js";

/** How often it is looked at: others are working in it, and so may a terminal be. */
const LOOK_EVERY = 5000;

/** Asks the host: what it says, or throws. `none`: there is no repository to ask. */
async function ask(entry, action, args = {}) {
  const r = await fetch(`git?project=${encodeURIComponent(entry)}`, { method: "POST", body: JSON.stringify({ action, ...args }) });
  if (!r.ok) throw new Error(`the host said ${r.status}`);
  return said(await r.json());
}
/** What the host answers, or throws what it says went wrong. */
function said(j) {
  if (j.stale) throw new Stale(j.error);
  if (j.error) throw new Cannot(j.error);
  return j;
}

export class HostRepo extends Repo {
  /** `entry`: the project it is the repository of. `who()`: who is making what is made here. */
  constructor({ entry, who = () => null }) {
    super();
    Object.assign(this, { entry, who });
  }
  ask(action, args) {
    return ask(this.entry, action, args);
  }
  get name() { return "The host"; }
  get pushes() { return false; }
  upstream(branch) { return `${branch} on the host`; }
  kindOf() { return "host"; }
  get joining() { return "They need to be able to reach this host."; }
  async head(branch) { return (await this.ask("head", { branch })).sha; }
  look(branch) { return this.head(branch); }
  readCommit(sha) { return this.ask("commitInfo", { sha }); }
  readTree(commit) { return this.ask("tree", { commit }); }
  async readBlob(sha) { return (await this.ask("blob", { sha })).text; }
  compare(from, to) { return this.ask("compare", { from, to }); }
  async log(from, count = 30) { return (await this.ask("log", { from, count })).commits; }
  async branches() { return (await this.ask("branches")).branches; }
  branch(name, at) { return this.ask("newBranch", { name, at }); }
  async commit(branch, expected, changes, message) {
    return (await this.ask("commitOn", { branch, expected, changes, message, who: this.who() })).sha;
  }
  /** (A merge is the working tree's, here: TreeWork.) */
  makeCommit() {
    throw new Refused("a merge is made in the host's working tree");
  }
  /** (Beside the app, as it is on the host: what is too big to be in a commit's text.) */
  bytes(id) {
    return { url: new URL(`files/${id.split("/").map(encodeURIComponent).join("/")}`, location.href).href };
  }
}

/** `askHost(action, args)`: what the session's room has it asked of the host there. */
export class GuestRepo extends HostRepo {
  constructor({ entry, askHost }) {
    super({ entry });
    this.askHost = askHost;
  }
  async ask(action, args) {
    return said(await this.askHost(action, args));
  }
  /** What is the host's person's to do. */
  get notHere() { return { commit: "only whoever invited you can commit: ask them to" }; }
  get joining() { return "They need nothing but the link; whoever shared it needs to have their host running."; }
  /** (Too large for the room, and not asked for: a photo or a recording is not read from here.) */
  bytes() { return null; }
}

export class TreeWork {
  /** The project's versions, where it is in a repository; else null. */
  static async of(entry, who) {
    const w = new TreeWork(entry, who), s = await ask(entry, "status").catch(() => null);
    if (!s || s.none) return null;
    w.last = s;
    w.git = new HostRepo({ entry, who });
    return w;
  }
  constructor(entry, who) {
    Object.assign(this, { entry, who, last: null, sig: "" });
  }
  ask(action, args) {
    return ask(this.entry, action, { who: this.who?.(), ...args });
  }
  /** What is said where something has been done: looked at again, and everyone told. */
  async done(r) {
    await this.status();
    return r;
  }
  can(action) {
    return ACTIONS.includes(action) && !(action in (this.last?.cannot ?? {}));
  }
  whyNot(action) {
    return this.last?.cannot?.[action] ?? `there is no git action ${action}`;
  }
  /** Looks at it again, now and then, for as long as the page is looked at. */
  watch() {
    setInterval(() => { if (!document.hidden) this.status().catch(() => {}); }, LOOK_EVERY);
  }
  async status() {
    const s = await this.ask("status");
    if (s.none) return this.last;
    this.last = s;
    const sig = JSON.stringify(s);
    if (sig !== this.sig) { this.sig = sig; workChanged(); }
    return { ...s, can: ACTIONS.filter((a) => this.can(a)) };
  }
  // (As a session is begun from it.)
  get sessionFrom() {
    return { branch: this.last.branch, commit: this.last.commit, git: this.git, name: this.last.repo };
  }
  async entries() {
    return (await this.ask("edits")).edits;
  }
  async edits() {
    return (await this.entries()).map((e) => e.id);
  }
  async fetch() {
    if (this.can("fetch")) await this.ask("fetch");
    await this.status();
    return this.last.head;
  }
  async pull() { return this.done(await this.ask("pull")); }
  async push() { return this.done(await this.ask("push")); }
  async diff(path) { return (await this.ask("diff", { path })).text; }
  async commit(message, paths) { return (await this.done(await this.ask("commit", { message, paths }))).sha; }
  async restore(paths) { return this.done(await this.ask("restore", { paths })); }
  async branches() { return (await this.ask("branches")).branches; }
  log(count = 30) { return this.ask("log", { count }); }
  async merge(name, message) { return this.done(await this.ask("merge", { name, message })); }
  async revert(commit) { return this.done(await this.ask("revert", { commit })); }
  async view(commit) { return this.done(await this.ask("checkout", { commit })); }
  async conflicts() { return this.ask("conflicts"); }
  async resolve(path, { side, text }) { return this.done(await this.ask("resolve", { path, side, text })); }
  async abort() { return this.done(await this.ask("abort")); }
  /** A new branch from here, with what is changed here, for everyone: the files stay as they are. */
  async newBranch(name) {
    await this.ask("branch", { name });
    await this.status();
    return { text: `created ${name}, and on it`, moved: true };
  }
  /** To a branch, for everyone: the host's files are that branch's. */
  async switchTo(name) {
    if (name === this.last.branch && !this.last.viewing) return { text: `already on ${name}` };
    await this.ask("switch", { name });
    await this.status();
    return { text: `on ${name}`, moved: true };
  }
  leaveView() {
    return this.switchTo(this.last.branch);
  }
}
