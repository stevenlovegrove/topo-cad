// A session (docs/collaboration.md): a room, what its log says, and who is in it. Everything that must arrive goes through the
// log, in the room's order. What is only true now (who is here, what they have open) is told to each peer as it changes, and
// again to whoever arrives; it is kept nowhere. What is only for showing goes peer to peer (motion.js).
//
// What the log has, each `{ k, … }`:
//   open   { place, name, branch, entry, commit, engine, host }   the first: what the session is of (`place`: the location; `name`: what it is
//          called; `engine`: the build of the app it was begun on; `host`: the host it is shared from, where it is shared over the internet)
//   ended  {}                                the host's person has stopped sharing it
//   hello  { who, name, picture }            someone is taken part, or is called something else now
//   edit   { file, over, gone | text | delta, build }   a file changed, from its version `over` (replica.js)
//   base   { commit }                        the files are now edits over this commit
//   say    { who, to, text }                 a message to someone
//   cannot { who, name, why }                someone invited cannot reach what the session is of
//
// Nothing here knows of a page: it runs in Node as well (web/tools/session-relay.mjs).

import { Room } from "../room.js";
import { Replica } from "./replica.js";

/** The colours people are told apart by, in the order they first come: dark enough for white on them, far apart in hue. */
export const COLOURS = ["#e5484d", "#2f7fe8", "#2e9e5b", "#b45bd6", "#e8820c", "#0ea5a5", "#d6409f", "#7a6ff0"];

/** How long the host is waited for to be here, and to answer. */
const HOST_WITHIN = 15000, ANSWER_WITHIN = 30000;
/** How often what each has of the files is compared, while they change: everyone's merges are to come to the same. */
const COMPARE_EVERY = 5000;

export class Session {
  /**
   * `relay`, `id`, `secret`: the room (room.js). `me`: `{ who, name, picture }`. `read(commit, file)`: a file as a commit has it (null:
   * none). `merge(file, base, ours, theirs)`: three ways (replica.js). `on`: `open(entry)`; `changed(file, { own, dropped, live, built })`: a file's log moved on; `base(commit)`;
   * `people()`: who is here, or what they said of themselves, has changed; `chat()`; `cannot({ who, name, why })`;
   * `refused(file, why)`: an edit of yours could not go; `signal(peer, value)`, `peers(list)`: for what goes peer to peer;
   * `caughtUp()`, `status(s)`, `gone()`: the relay has forgotten it.
   */
  constructor({ relay, id, secret, me, read, merge, on = {}, create = false }) {
    Object.assign(this, { me, read, on, id, secret });
    this.opened = null;
    /** The commit the files are over, at this place in the log. */
    this.commit = null;
    this.replica = new Replica((file) => read(this.commit, file), merge, () => this.room.newId());
    /** The files found not to be the same as someone else's. */
    this.diverged = new Set();
    this.participants = new Map();
    this.messages = [];
    /** Who is here now: relay's peer → what they last said of themselves. */
    this.here = new Map();
    this.mine = {};
    /** One edit made at a time: the file may have to be read first. */
    this.editing = Promise.resolve();
    /** What each edit waiting to be heard back from is of. */
    this.fileOfEdit = new Map();
    this.status = "connecting";
    this.unheard = [];
    /** What has been asked of the host, and what of each answer has come: id → `{ done, fail, parts, timer }`. */
    this.asked = new Map();
    this.caughtUpOnce = new Promise((done) => { this.caught = done; });
    this.room = new Room({ relay, id, secret, on: {
      entry: (e) => this.heard(e),
      peers: (list) => this.peersAre(list),
      signal: (from, value) => this.signalled(from, value),
      refused: (eid, why) => this.refused(eid, why),
      status: (s) => { this.status = s; this.on.status?.(s); },
      caughtUp: () => { this.caught(); this.on.caughtUp?.(); if (!create) this.greet(); },
      forgotten: () => this.on.gone?.(),
      unreadable: () => {},
    } });
  }
  /** This browser's own id for this connection. */
  get peer() { return this.room.peer; }

  // ----- the log --------------------------------------------------------------------------------------------------------

  async heard({ value: v, at, id }) {
    if (v.k === "open") {
      this.opened ??= v;
      this.commit ??= v.commit;
      return this.on.open?.(v);
    }
    if (v.k === "hello") {
      const had = this.participants.get(v.who);
      this.participants.set(v.who, { ...had, id: v.who, name: v.name, picture: v.picture ?? null, kind: "person", colour: had?.colour ?? COLOURS[this.participants.size % COLOURS.length], since: had?.since ?? at });
      return this.on.people?.();
    }
    if (v.k === "edit") {
      await this.replica.ensure(v.file);
      const was = { live: this.replica.live(v.file), built: this.replica.built(v.file) };
      const { own, dropped, next, mine, base } = this.replica.heard({ ...v, id });
      this.fileOfEdit.delete(id);
      if (next) this.sayEdit(next);
      this.compareSoon();
      return this.on.changed?.(v.file, { own, dropped, mine, base, live: this.replica.live(v.file), built: this.replica.built(v.file), liveWas: was.live, builtWas: was.built });
    }
    if (v.k === "base") {
      this.commit = v.commit;
      return this.on.base?.(v.commit);
    }
    if (v.k === "say") {
      this.messages.push({ from: v.who, to: v.to, text: v.text, time: at });
      return this.on.chat?.();
    }
    if (v.k === "cannot") return this.on.cannot?.(v);
    if (v.k === "ended") {
      this.ended = true;
      return this.on.ended?.();
    }
  }
  refused(id, why) {
    const file = this.fileOfEdit.get(id);
    if (!file) return;
    this.fileOfEdit.delete(id);
    const { live, next } = this.replica.forget(id, file);
    if (next) this.sayEdit(next);
    this.on.refused?.(file, why, live);
  }

  /** Makes a file this text, seen at once, the log told. `build`: the model is to be built from it (not while it is typed). `base`:
   *  the text it was made from (Replica.edit). */
  edit(file, text, { build = true, base } = {}) {
    const r = (this.editing = this.editing.catch(() => {}).then(async () => {
      await this.replica.ensure(file);
      const e = this.replica.edit(file, text, { build, base });
      if (!e) return false;
      if (e.clash) {
        this.on.refused?.(file, "clash", this.replica.live(file), { mine: text, base });
        return false;
      }
      this.sayEdit(e);
      return true;
    }));
    return r;
  }
  sayEdit({ id, ...said }) {
    this.fileOfEdit.set(id, said.file);
    this.room.say({ k: "edit", ...said }, id);
  }
  /** Into the log as it is: `{ k, … }`. */
  say(value) {
    return this.room.say(value);
  }
  /** Whether everything said here has been heard back from. */
  get settled() {
    return this.replica.settled && this.room.waiting.size === 0;
  }

  // ----- who is here -----------------------------------------------------------------------------------------------------

  /** Said to the log, once: who this is, or what they are called now. */
  greet() {
    const known = this.participants.get(this.me.who);
    if (!known || known.name !== this.me.name || known.picture !== (this.me.picture ?? null)) this.say({ k: "hello", ...this.me });
    this.tellEveryone();
  }
  rename(name) {
    this.me = { ...this.me, name };
    this.greet();
  }
  /** Something of what is true of this person now, for those here. (A change at a time goes as one, at the end of the turn.) */
  present(patch) {
    Object.assign(this.mine, patch);
    if (this.telling) return;
    this.telling = true;
    queueMicrotask(() => { this.telling = false; this.tellEveryone(); });
  }
  tellEveryone() {
    for (const peer of this.here.keys()) this.tell(peer);
  }
  tell(peer) {
    this.room.signal(peer, { here: { who: this.me.who, ...this.mine } });
  }
  peersAre(list) {
    const now = new Set(list);
    for (const peer of this.here.keys()) if (!now.has(peer)) this.here.delete(peer);
    for (const peer of list) if (!this.here.has(peer)) { this.here.set(peer, {}); this.tell(peer); }
    this.on.peers?.(list);
    this.on.people?.();
  }
  signalled(from, value) {
    if (value.told) return this.toldByHost(value.told);
    if (value.here) {
      if (!this.here.has(from)) return;
      this.here.set(from, value.here);
      this.compare();
      return this.on.people?.();
    }
    // (Those that come before anyone is listening wait: a first offer lost is a connection never made.)
    if (this.on.signal) this.on.signal(from, value);
    else this.unheard.push([from, value]);
  }
  // ----- whether everyone's merges came to the same -------------------------------------------------------------------------

  /** Says what this person has of the files, a while after they last changed. */
  compareSoon() {
    this.comparing ??= setTimeout(() => { this.comparing = null; this.present({ sums: this.replica.sums() }); this.compare(); }, COMPARE_EVERY);
  }
  /** Of what those here say they have: told once of each file that differs. (Two builds of the app whose merges are not the same would do it.) */
  compare() {
    for (const said of this.here.values()) {
      for (const file of this.replica.differs(said.sums)) {
        if (this.diverged.has(file)) continue;
        this.diverged.add(file);
        this.on.diverged?.(file);
      }
    }
  }

  // ----- the host, where it is shared over the internet (crates/topo-cli/src/share.rs) --------------------------------------

  /** Those here who are people, by their peers. (A host is here too, and is nobody.) */
  others() {
    return [...this.here].filter(([, said]) => !said.service).map(([peer]) => peer);
  }
  /** Something asked of the host, by the room: what it answers. */
  async askHost(action, args = {}) {
    let peer;
    for (const end = Date.now() + HOST_WITHIN; !(peer = this.peerOf("host"));) {
      if (Date.now() > end) throw new Error("the host this session is shared from is not connected: whoever shared it needs to have topo serve running");
      await new Promise((r) => setTimeout(r, 50));
    }
    const id = this.room.newId();
    return new Promise((done, fail) => {
      const timer = setTimeout(() => { this.asked.delete(id); fail(new Error("the host this session is shared from did not answer")); }, ANSWER_WITHIN);
      this.asked.set(id, { done, parts: [], timer });
      this.room.signal(peer, { ask: { id, action, args } });
    });
  }
  toldByHost({ id, part, of, text }) {
    const a = this.asked.get(id);
    if (!a) return;
    a.parts[part] = text;
    if (a.parts.filter((p) => p != null).length < of) return;
    this.asked.delete(id);
    clearTimeout(a.timer);
    a.done(JSON.parse(a.parts.join("")));
  }

  /** `f(peer, value)` for what peers send each other, from now on and what has come already. */
  listen(f) {
    this.on.signal = f;
    for (const [from, value] of this.unheard.splice(0)) f(from, value);
  }
  /** The peer of someone here (by who they are), or undefined. */
  peerOf(who) {
    for (const [peer, said] of this.here) if (said.who === who) return peer;
    return undefined;
  }

  /** Everyone who has taken part, as a workspace's participants are: `presence` "here" or "away". */
  people() {
    const on = new Set([this.me.who, ...[...this.here.values()].map((s) => s.who)]);
    return [...this.participants.values()].map((p) => ({ ...p, presence: on.has(p.id) ? "here" : "away" }));
  }

  close() {
    clearTimeout(this.comparing);
    this.room.close();
  }
}
