// A project in a session, as a store: the branch's files at the commit the session is at, and over them what the log says.
// Whatever is written goes to the log (session.js), and what the log says is written here, so the rest of the page reads and
// builds as it would of any project on a branch, and commits the same way. Kept apart from the person's own edits of the branch.

import { BranchWork, Cannot, merger } from "../work.js";

/** What each browser makes for itself. (Your own layer, `*.user.gen.ts`, is shared while in a session: everyone has one model.) */
const OWN = /\.(solved|drawn)\.json$/;
/** What a session is not for: it is of one line of work, and what is in it is everyone's. */
const NOT_IN_A_SESSION = {
  merge: "leave the session to merge: a session is of one line of work",
  revert: "leave the session to revert a commit",
  checkout: "leave the session to check out another commit",
  branch: "leave the session to create a branch",
  switch: "leave the session to switch to another branch",
  session: "this is a session already",
  share: "this is a session already: its link is in the header's menu",
};

export class SessionWork extends BranchWork {
  /** `changed(file)`: what the model is built from has changed here, by someone else. */
  constructor(base, top, session) {
    super(base, top);
    this.session = session;
  }
  can(action) {
    return !(action in NOT_IN_A_SESSION) && !(action in (this.base.git.notHere ?? {})) && super.can(action);
  }
  whyNot(action) {
    return NOT_IN_A_SESSION[action] ?? this.base.git.notHere?.[action] ?? super.whyNot(action);
  }
  async status() {
    return { ...(await super.status()), session: true };
  }
  /** A file as the log, with this person's edits over it, says the model is to be built from: kept for reading. */
  async keepBuilt(file) {
    await this.keep(file, this.session.replica.built(file));
  }
  /** `base`: the text this was made from, where it may be older than the file as it now is: what others have done since stays. */
  write(id, text, { base } = {}) {
    if (OWN.test(id)) return this.top.write(id, text);
    return this.queue(async () => {
      const was = await this.read(id);
      await this.session.edit(id, text, { build: true, base });
      await this.keepBuilt(id);
      const now = await this.read(id);
      if (was !== now) {
        this.undos.push({ id, was, now });
        this.redos = [];
      }
      this.said();
    });
  }
  /** Taking back is an edit like any other: of what is there now, so it goes where it can, or is dropped. */
  step(from, to, side) {
    return this.queue(async () => {
      const s = from.pop();
      if (!s) return false;
      await this.session.edit(s.id, s[side], { build: true, base: side === "was" ? s.now : s.was });
      await this.keepBuilt(s.id);
      to.push(s);
      this.said();
      return true;
    });
  }
  restore(paths) {
    return this.queue(async () => {
      const ids = paths ?? (await this.edits());
      for (const id of ids) {
        await this.session.edit(id, await this.base.read(id), { build: true });
        await this.keepBuilt(id);
      }
      this.undos = this.undos.filter((u) => !ids.includes(u.id));
      this.redos = this.redos.filter((u) => !ids.includes(u.id));
      this.said();
    });
  }
  /** An edit of this person's that was not kept, to have back: in conflict as anything brought in is, and to be settled as that is (compared, a
   *  side taken, or edited by hand), or let go of. `mine`: what they had; `now`: what the file is; `base`: what it was made from. Nothing else waits
   *  for it. (Not while something else is being settled: the log has it still.) */
  async notKept(file, { mine, now, base }) {
    if (this.clash && !this.clash.optional) return;
    const lib = await merger();
    const c = (this.clash ??= { doing: "an edit of yours was not kept", optional: true, paths: [], ours: {}, theirs: {}, marked: {}, chosen: {}, writes: {}, finish: async (all) => {
      for (const [id, text] of Object.entries(all)) {
        await this.session.edit(id, text, { build: true, base: c.theirs[id] });
        await this.keepBuilt(id);
      }
      this.said();
      return { text: "put back, as far as it goes" };
    } });
    if (!c.paths.includes(file)) c.paths.push(file);
    delete c.chosen[file];
    c.ours[file] = mine;
    c.theirs[file] = now;
    c.marked[file] = mine != null && now != null ? lib.merge_marked(file, base ?? "", mine, now) : mine ?? now;
    this.said();
  }
  /** A commit, of what the model is built from; then everyone is over it. */
  async commit(message, paths) {
    const sha = await super.commit(message, paths);
    await this.session.say({ k: "base", commit: sha });
    return sha;
  }
  /** Where the branch has got to brought in, what clashes with the session settled first; then everyone is over it. */
  async pull() {
    await this.mustBeClear("pull", { edits: false });
    const { base } = this, head = await this.fetch();
    if (head === base.at.commit) return { pulled: false };
    const { files: incoming, texts } = await this.withEdits();
    const { writes, clash } = await this.combine({ base: base.at.files, incoming, onto: (await base.treeOf(head)).files, texts, ours: "incoming" });
    return this.settle({
      doing: `pulling ${base.branch}`,
      writes,
      clash,
      ours: "incoming",
      finish: async (all) => {
        for (const [id, text] of Object.entries(all)) await this.session.edit(id, text, { build: true, base: texts[id] });
        await this.session.say({ k: "base", commit: head });
        // (Here at once, as anything this person does is; the log's word on it comes back and finds it done.)
        await base.committedAs(head);
        return { pulled: true };
      },
    });
  }
  /** The log says the files are over this commit now: what is kept is what differs from it. */
  moved(commit) {
    return this.queue(async () => {
      if (this.base.at?.commit !== commit) await this.base.committedAs(commit);
      for (const file of this.session.replica.touched) await this.keepBuilt(file);
      this.said();
    });
  }
  /** What the log says the model is built from has moved on, for this file. */
  arrived(file) {
    return this.queue(async () => {
      await this.keepBuilt(file);
      this.said();
    });
  }
  discardEdits() { throw new Cannot("a session's changes are everyone's: restore them, which they see"); }
  abort() {
    if (this.clash) return super.abort();
    throw new Cannot("no pull is in progress");
  }
}
